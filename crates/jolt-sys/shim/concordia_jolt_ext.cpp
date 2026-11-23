// Concordia's C API over Jolt Physics: init with counted allocations, a deterministic world,
// bodies, stepping, batched contact events, raycasts, state save/restore and virtual characters.

#include "concordia_jolt_ext.h"

#include <algorithm>
#include <atomic>
#include <map>
#include <mutex>
#include <string>
#include <unordered_set>
#include <vector>

#include <Jolt/Jolt.h>

#include <Jolt/Core/Factory.h>
#include <Jolt/Core/JobSystemThreadPool.h>
#include <Jolt/Core/Memory.h>
#include <Jolt/Core/TempAllocator.h>
#include <Jolt/Physics/Body/BodyCreationSettings.h>
#include <Jolt/Physics/Body/BodyFilter.h>
#include <Jolt/Physics/Character/CharacterVirtual.h>
#include <Jolt/Physics/Collision/CastResult.h>
#include <Jolt/Physics/Collision/ContactListener.h>
#include <Jolt/Physics/Collision/RayCast.h>
#include <Jolt/Physics/Collision/Shape/BoxShape.h>
#include <Jolt/Physics/Collision/Shape/CapsuleShape.h>
#include <Jolt/Physics/Collision/Shape/CylinderShape.h>
#include <Jolt/Physics/Collision/Shape/RotatedTranslatedShape.h>
#include <Jolt/Physics/Collision/Shape/SphereShape.h>
#include <Jolt/Physics/PhysicsSystem.h>
#include <Jolt/Physics/StateRecorderImpl.h>
#include <Jolt/RegisterTypes.h>

using namespace JPH;

namespace {

// ---- counted allocations -------------------------------------------------

std::atomic<int64_t> g_live{0};
AllocateFunction g_allocate = nullptr;
ReallocateFunction g_reallocate = nullptr;
FreeFunction g_free = nullptr;
AlignedAllocateFunction g_aligned_allocate = nullptr;
AlignedFreeFunction g_aligned_free = nullptr;

void* counted_allocate(size_t size) {
    void* block = g_allocate(size);
    if (block != nullptr) g_live.fetch_add(1, std::memory_order_relaxed);
    return block;
}

void* counted_reallocate(void* block, size_t old_size, size_t new_size) {
    void* moved = g_reallocate(block, old_size, new_size);
    if (block == nullptr && moved != nullptr) g_live.fetch_add(1, std::memory_order_relaxed);
    return moved;
}

void counted_free(void* block) {
    if (block != nullptr) g_live.fetch_sub(1, std::memory_order_relaxed);
    g_free(block);
}

void* counted_aligned_allocate(size_t size, size_t alignment) {
    void* block = g_aligned_allocate(size, alignment);
    if (block != nullptr) g_live.fetch_add(1, std::memory_order_relaxed);
    return block;
}

void counted_aligned_free(void* block) {
    if (block != nullptr) g_live.fetch_sub(1, std::memory_order_relaxed);
    g_aligned_free(block);
}

// ---- layers: object layer = user layer * 2 + moving bit --------------------

constexpr uint32_t kBroadNonMoving = 0;
constexpr uint32_t kBroadMoving = 1;

ObjectLayer object_layer(uint8_t layer, bool moving) {
    return static_cast<ObjectLayer>(layer * 2 + (moving ? 1 : 0));
}

uint8_t user_layer(ObjectLayer layer) { return static_cast<uint8_t>(layer / 2); }
bool is_moving(ObjectLayer layer) { return (layer & 1) != 0; }

class BroadPhaseLayers final : public BroadPhaseLayerInterface {
public:
    uint GetNumBroadPhaseLayers() const override { return 2; }
    BroadPhaseLayer GetBroadPhaseLayer(ObjectLayer layer) const override {
        return BroadPhaseLayer(is_moving(layer) ? kBroadMoving : kBroadNonMoving);
    }
#if defined(JPH_EXTERNAL_PROFILE) || defined(JPH_PROFILE_ENABLED)
    const char* GetBroadPhaseLayerName(BroadPhaseLayer layer) const override {
        return layer.GetValue() == kBroadMoving ? "moving" : "non-moving";
    }
#endif
};

class ObjectVsBroad final : public ObjectVsBroadPhaseLayerFilter {
public:
    bool ShouldCollide(ObjectLayer layer, BroadPhaseLayer broad) const override {
        return is_moving(layer) || broad.GetValue() == kBroadMoving;
    }
};

class LayerPairs final : public ObjectLayerPairFilter {
public:
    explicit LayerPairs(const uint16_t* collide) { std::copy(collide, collide + CJ_LAYERS, mCollide); }
    bool ShouldCollide(ObjectLayer a, ObjectLayer b) const override {
        if (!is_moving(a) && !is_moving(b)) return false;
        return (mCollide[user_layer(a)] >> user_layer(b)) & 1;
    }

private:
    uint16_t mCollide[CJ_LAYERS] = {};
};

class MaskFilter final : public ObjectLayerFilter {
public:
    explicit MaskFilter(uint16_t mask) : mMask(mask) {}
    bool ShouldCollide(ObjectLayer layer) const override { return (mMask >> user_layer(layer)) & 1; }

private:
    uint16_t mMask;
};

// ---- contact events: one begin/end per body pair, collected under a lock ----

class ContactLog final : public ContactListener {
public:
    /// Characters' inner bodies (changed only between steps; read during them).
    void AddInner(BodyID id) { mInner.insert(id.GetIndexAndSequenceNumber()); }
    void RemoveInner(BodyID id) { mInner.erase(id.GetIndexAndSequenceNumber()); }
    bool IsInner(BodyID id) const { return mInner.count(id.GetIndexAndSequenceNumber()) != 0; }

    /// A character's inner body exists for sensors: every other pair with it is rejected.
    ValidateResult OnContactValidate(const Body& b1, const Body& b2, RVec3Arg, const CollideShapeResult&) override {
        bool inner_vs_solid = (IsInner(b1.GetID()) && !b2.IsSensor()) || (IsInner(b2.GetID()) && !b1.IsSensor());
        return inner_vs_solid ? ValidateResult::RejectAllContactsForThisBodyPair
                              : ValidateResult::AcceptAllContactsForThisBodyPair;
    }

    void OnContactAdded(const Body& b1, const Body& b2, const ContactManifold&, ContactSettings&) override {
        std::lock_guard<std::mutex> lock(mMutex);
        auto key = Key(b1.GetID(), b2.GetID());
        Pair& pair = mPairs[key];
        if (pair.count++ == 0) {
            bool first = b1.GetID() < b2.GetID();
            pair.a = first ? b1.GetUserData() : b2.GetUserData();
            pair.b = first ? b2.GetUserData() : b1.GetUserData();
            pair.sensor = b1.IsSensor() || b2.IsSensor();
            mEvents.push_back({pair.a, pair.b, true, pair.sensor});
        }
    }

    void OnContactRemoved(const SubShapeIDPair& shapes) override {
        std::lock_guard<std::mutex> lock(mMutex);
        auto it = mPairs.find(Key(shapes.GetBody1ID(), shapes.GetBody2ID()));
        if (it == mPairs.end()) return;
        if (--it->second.count == 0) {
            mEvents.push_back({it->second.a, it->second.b, false, it->second.sensor});
            mPairs.erase(it);
        }
    }

    size_t Drain(CjContactEvent* out, size_t cap) {
        std::lock_guard<std::mutex> lock(mMutex);
        size_t n = std::min(cap, mEvents.size());
        std::copy(mEvents.begin(), mEvents.begin() + static_cast<std::ptrdiff_t>(n), out);
        mEvents.erase(mEvents.begin(), mEvents.begin() + static_cast<std::ptrdiff_t>(n));
        return n;
    }

    size_t Pending() {
        std::lock_guard<std::mutex> lock(mMutex);
        return mEvents.size();
    }

private:
    struct Pair {
        uint32_t count = 0;
        uint64_t a = 0;
        uint64_t b = 0;
        bool sensor = false;
    };
    static uint64_t Key(BodyID x, BodyID y) {
        uint32_t lo = std::min(x.GetIndexAndSequenceNumber(), y.GetIndexAndSequenceNumber());
        uint32_t hi = std::max(x.GetIndexAndSequenceNumber(), y.GetIndexAndSequenceNumber());
        return (static_cast<uint64_t>(lo) << 32) | hi;
    }
    std::unordered_set<uint32_t> mInner;
    std::mutex mMutex;
    std::map<uint64_t, Pair> mPairs;
    std::vector<CjContactEvent> mEvents;
};

Vec3 vec3(const float v[3]) { return Vec3(v[0], v[1], v[2]); }
Quat quat(const float q[4]) { return Quat(q[0], q[1], q[2], q[3]).Normalized(); }

void store(Vec3Arg v, float out[3]) {
    out[0] = v.GetX();
    out[1] = v.GetY();
    out[2] = v.GetZ();
}

void store(QuatArg q, float out[4]) {
    out[0] = q.GetX();
    out[1] = q.GetY();
    out[2] = q.GetZ();
    out[3] = q.GetW();
}

bool positive(float x) { return x > 0.0f && x < 1.0e6f; }

RefConst<Shape> make_shape(const CjBodyDesc& d) {
    RefConst<Shape> base;
    switch (d.shape) {
        case CJ_SHAPE_BOX:
            if (!positive(d.dims[0]) || !positive(d.dims[1]) || !positive(d.dims[2])) return nullptr;
            base = new BoxShape(vec3(d.dims), std::min({0.05f, d.dims[0], d.dims[1], d.dims[2]}) * 0.5f);
            break;
        case CJ_SHAPE_SPHERE:
            if (!positive(d.dims[0])) return nullptr;
            base = new SphereShape(d.dims[0]);
            break;
        case CJ_SHAPE_CAPSULE:
            if (!positive(d.dims[0]) || !positive(d.dims[1])) return nullptr;
            base = new CapsuleShape(d.dims[0], d.dims[1]);
            break;
        case CJ_SHAPE_CYLINDER:
            if (!positive(d.dims[0]) || !positive(d.dims[1])) return nullptr;
            base = new CylinderShape(d.dims[0], d.dims[1], std::min(0.05f, std::min(d.dims[0], d.dims[1])) * 0.5f);
            break;
        default:
            return nullptr;
    }
    Vec3 offset = vec3(d.offset);
    if (offset.IsNearZero()) return base;
    return new RotatedTranslatedShape(offset, Quat::sIdentity(), base);
}

EMotionType motion(CjMotion m) {
    switch (m) {
        case CJ_MOTION_KINEMATIC: return EMotionType::Kinematic;
        case CJ_MOTION_DYNAMIC: return EMotionType::Dynamic;
        default: return EMotionType::Static;
    }
}

}  // namespace

/// Skips characters' inner bodies in queries (they exist for sensors only).
class NotInner final : public BodyFilter {
public:
    explicit NotInner(const ContactLog& contacts) : mContacts(contacts) {}
    bool ShouldCollide(const BodyID& id) const override { return !mContacts.IsInner(id); }

private:
    const ContactLog& mContacts;
};

struct CjWorld {
    BroadPhaseLayers broad;
    ObjectVsBroad object_vs_broad;
    LayerPairs pairs;
    ContactLog contacts;
    TempAllocatorImpl temp{16 * 1024 * 1024};
    JobSystemThreadPool jobs;
    PhysicsSystem system;

    explicit CjWorld(const CjWorldDesc& d)
        : pairs(d.collide), jobs(cMaxPhysicsJobs, cMaxPhysicsBarriers, static_cast<int>(d.threads) - 1) {
        system.Init(d.max_bodies, 0, d.max_bodies, d.max_bodies, broad, object_vs_broad, pairs);
        system.SetGravity(vec3(d.gravity));
        system.SetContactListener(&contacts);
    }
};

struct CjCharacter {
    Ref<CharacterVirtual> inner;
    CjWorld* world;
    uint8_t layer;
    float step_height;
};

extern "C" {

void concordia_jolt_version(uint32_t* major, uint32_t* minor, uint32_t* patch) {
    if (major) *major = JPH_VERSION_MAJOR;
    if (minor) *minor = JPH_VERSION_MINOR;
    if (patch) *patch = JPH_VERSION_PATCH;
}

bool concordia_jolt_init(void) {
    static std::once_flag once;
    std::call_once(once, [] {
        RegisterDefaultAllocator();
        g_allocate = Allocate;
        g_reallocate = Reallocate;
        g_free = Free;
        g_aligned_allocate = AlignedAllocate;
        g_aligned_free = AlignedFree;
        Allocate = counted_allocate;
        Reallocate = counted_reallocate;
        Free = counted_free;
        AlignedAllocate = counted_aligned_allocate;
        AlignedFree = counted_aligned_free;
        Factory::sInstance = new Factory();
        RegisterTypes();
    });
    return Factory::sInstance != nullptr;
}

int64_t concordia_jolt_live_allocations(void) { return g_live.load(std::memory_order_relaxed); }

CjWorld* cj_world_create(const CjWorldDesc* desc) {
    if (desc == nullptr || desc->max_bodies == 0 || desc->max_bodies > 65536 || desc->threads == 0 ||
        desc->threads > 64 || !concordia_jolt_init())
        return nullptr;
    return new CjWorld(*desc);
}

void cj_world_destroy(CjWorld* world) { delete world; }

uint32_t cj_body_add(CjWorld* world, const CjBodyDesc* d) {
    if (world == nullptr || d == nullptr || d->layer >= CJ_LAYERS) return CJ_NO_BODY;
    RefConst<Shape> shape = make_shape(*d);
    if (shape == nullptr) return CJ_NO_BODY;
    bool moving = d->motion != CJ_MOTION_STATIC;
    BodyCreationSettings s(shape, vec3(d->position), quat(d->rotation), motion(d->motion),
                           object_layer(d->layer, moving));
    s.mUserData = d->user;
    s.mIsSensor = d->sensor;
    s.mCollideKinematicVsNonDynamic = d->sensor;
    s.mMotionQuality = d->ccd ? EMotionQuality::LinearCast : EMotionQuality::Discrete;
    s.mFriction = d->friction;
    s.mRestitution = d->restitution;
    s.mLinearDamping = d->linear_damping;
    s.mAngularDamping = d->angular_damping;
    s.mGravityFactor = d->gravity_scale;
    s.mLinearVelocity = vec3(d->linear_velocity);
    s.mAngularVelocity = vec3(d->angular_velocity);
    if (d->mass > 0.0f) {
        s.mOverrideMassProperties = EOverrideMassProperties::CalculateInertia;
        s.mMassPropertiesOverride.mMass = d->mass;
    }
    EActivation activation = moving ? EActivation::Activate : EActivation::DontActivate;
    BodyID id = world->system.GetBodyInterface().CreateAndAddBody(s, activation);
    return id.IsInvalid() ? CJ_NO_BODY : id.GetIndexAndSequenceNumber();
}

void cj_body_remove(CjWorld* world, uint32_t body) {
    if (world == nullptr || body == CJ_NO_BODY) return;
    BodyInterface& bodies = world->system.GetBodyInterface();
    BodyID id(body);
    if (!bodies.IsAdded(id)) return;
    bodies.RemoveBody(id);
    bodies.DestroyBody(id);
}

bool cj_body_state(CjWorld* world, uint32_t body, CjBodyState* out) {
    if (world == nullptr || out == nullptr || body == CJ_NO_BODY) return false;
    BodyInterface& bodies = world->system.GetBodyInterface();
    BodyID id(body);
    if (!bodies.IsAdded(id)) return false;
    RVec3 position;
    Quat rotation;
    bodies.GetPositionAndRotation(id, position, rotation);
    store(Vec3(position), out->position);
    store(rotation, out->rotation);
    store(bodies.GetLinearVelocity(id), out->linear_velocity);
    store(bodies.GetAngularVelocity(id), out->angular_velocity);
    return true;
}

void cj_body_move_kinematic(CjWorld* world, uint32_t body, const float position[3], const float rotation[4], float dt) {
    if (world == nullptr || body == CJ_NO_BODY || !(dt > 0.0f)) return;
    world->system.GetBodyInterface().MoveKinematic(BodyID(body), vec3(position), quat(rotation), dt);
}

uint32_t cj_world_step(CjWorld* world, float dt, int32_t collision_steps) {
    if (world == nullptr || !(dt > 0.0f) || collision_steps < 1) return 0xffffffffu;
    return static_cast<uint32_t>(world->system.Update(dt, collision_steps, &world->temp, &world->jobs));
}

size_t cj_world_drain_events(CjWorld* world, CjContactEvent* out, size_t cap) {
    if (world == nullptr || out == nullptr) return 0;
    return world->contacts.Drain(out, cap);
}

size_t cj_world_pending_events(CjWorld* world) { return world == nullptr ? 0 : world->contacts.Pending(); }

bool cj_raycast(CjWorld* world, const float origin[3], const float dir_len[3], uint16_t layer_mask, CjRayHit* out) {
    if (world == nullptr || out == nullptr) return false;
    RRayCast ray{vec3(origin), vec3(dir_len)};
    RayCastResult hit;
    MaskFilter filter(layer_mask);
    NotInner bodies(world->contacts);
    if (!world->system.GetNarrowPhaseQuery().CastRay(ray, hit, {}, filter, bodies)) return false;
    BodyLockRead lock(world->system.GetBodyLockInterface(), hit.mBodyID);
    if (!lock.Succeeded()) return false;
    const Body& body = lock.GetBody();
    RVec3 point = ray.GetPointOnRay(hit.mFraction);
    out->user = body.GetUserData();
    out->fraction = hit.mFraction;
    store(Vec3(point), out->point);
    store(body.GetWorldSpaceSurfaceNormal(hit.mSubShapeID2, point), out->normal);
    return true;
}

size_t cj_world_save(CjWorld* world, uint8_t* out, size_t cap) {
    if (world == nullptr) return 0;
    StateRecorderImpl recorder;
    world->system.SaveState(recorder);
    std::string data = recorder.GetData();
    if (out != nullptr && cap >= data.size()) std::copy(data.begin(), data.end(), out);
    return data.size();
}

bool cj_world_restore(CjWorld* world, const uint8_t* data, size_t len) {
    if (world == nullptr || data == nullptr) return false;
    StateRecorderImpl recorder;
    recorder.WriteBytes(data, len);
    return world->system.RestoreState(recorder);
}

CjCharacter* cj_character_create(CjWorld* world, const CjCharacterDesc* d) {
    if (world == nullptr || d == nullptr || d->layer >= CJ_LAYERS || !positive(d->radius) ||
        !(d->height > 2.0f * d->radius) || !(d->max_slope_radians > 0.0f) || d->step_height < 0.0f)
        return nullptr;
    float half_cylinder = 0.5f * d->height - d->radius;
    RefConst<Shape> capsule = new CapsuleShape(half_cylinder, d->radius);
    Ref<CharacterVirtualSettings> settings = new CharacterVirtualSettings();
    settings->mShape = new RotatedTranslatedShape(Vec3(0, 0.5f * d->height, 0), Quat::sIdentity(), capsule);
    settings->mMaxSlopeAngle = d->max_slope_radians;
    settings->mSupportingVolume = Plane(Vec3::sAxisY(), -d->radius);
    settings->mInnerBodyShape = settings->mShape;
    settings->mInnerBodyLayer = object_layer(d->layer, true);
    auto* character = new CjCharacter();
    character->inner = new CharacterVirtual(settings, vec3(d->position), Quat::sIdentity(), d->user, &world->system);
    character->world = world;
    world->contacts.AddInner(character->inner->GetInnerBodyID());
    character->layer = d->layer;
    character->step_height = d->step_height;
    return character;
}

void cj_character_destroy(CjCharacter* character) {
    if (character == nullptr) return;
    character->world->contacts.RemoveInner(character->inner->GetInnerBodyID());
    delete character;
}

void cj_character_move(CjWorld* world, CjCharacter* c, const float velocity[3], float dt) {
    if (world == nullptr || c == nullptr || !(dt > 0.0f)) return;
    Vec3 gravity = world->system.GetGravity();
    Vec3 current = c->inner->GetLinearVelocity();
    bool grounded = c->inner->GetGroundState() == CharacterBase::EGroundState::OnGround;
    float vertical = grounded ? 0.0f : current.GetY() + gravity.GetY() * dt;
    c->inner->SetLinearVelocity(Vec3(velocity[0], vertical, velocity[2]));
    CharacterVirtual::ExtendedUpdateSettings settings;
    settings.mWalkStairsStepUp = Vec3(0, c->step_height, 0);
    ObjectLayer layer = object_layer(c->layer, true);
    DefaultBroadPhaseLayerFilter broad(world->object_vs_broad, layer);
    DefaultObjectLayerFilter objects(world->pairs, layer);
    NotInner bodies(world->contacts);  // characters never collide with each other's inner bodies
    c->inner->ExtendedUpdate(dt, gravity, settings, broad, objects, bodies, {}, world->temp);
}

void cj_character_position(const CjCharacter* c, float out[3]) {
    if (c == nullptr || out == nullptr) return;
    store(Vec3(c->inner->GetPosition()), out);
}

bool cj_character_on_ground(const CjCharacter* c) {
    return c != nullptr && c->inner->GetGroundState() == CharacterBase::EGroundState::OnGround;
}

}  // extern "C"
