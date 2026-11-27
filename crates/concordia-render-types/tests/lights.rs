//! Light definitions both renderers use (M4 4.1b2): falloff windowed to zero at `range`, the spot
//! cone, and the sky's equirect mapping.

use concordia_core::math::Vec3;
use concordia_render_types::{SpotCone, distance_window, equirect};

#[test]
fn falloff_is_inverse_square_near_and_zero_at_range() {
    let near = distance_window(0.01, 10.0);
    assert!((near - 100.0).abs() < 0.01, "1/d² at d = 0.1: {near}");
    assert_eq!(distance_window(100.0, 10.0), 0.0, "zero at range");
    assert_eq!(distance_window(400.0, 10.0), 0.0, "zero beyond");
    let half = distance_window(25.0, 10.0);
    let window = (1.0f32 - 0.25f32 * 0.25).powi(2);
    assert!((half - window / 25.0).abs() < 1e-6, "{half}");
    assert!(
        distance_window(0.0, 10.0).is_finite(),
        "no division by zero"
    );
}

#[test]
fn spot_cone_is_full_inside_zero_outside_and_smooth_between() {
    let cone = SpotCone::new(0.3, 0.5);
    assert_eq!(cone.attenuation(0.2f32.cos()), 1.0);
    assert_eq!(cone.attenuation(0.6f32.cos()), 0.0);
    let mid = cone.attenuation(0.4f32.cos());
    assert!(mid > 0.0 && mid < 1.0, "{mid}");
    let (scale, offset) = (cone.scale, cone.offset);
    let t: f32 = (0.4f32.cos() * scale + offset).clamp(0.0, 1.0);
    assert_eq!(mid, t * t, "saturate(cos · scale + offset)²");
    assert!(
        SpotCone::new(0.5, 0.5).scale.is_finite(),
        "equal angles stay finite"
    );
}

#[test]
fn equirect_mapping_round_trips_with_its_landmarks() {
    let [u, v] = equirect::dir_to_uv(Vec3::NEG_Z);
    assert!(
        (u - 0.5).abs() < 1e-6 && (v - 0.5).abs() < 1e-6,
        "-Z is the center"
    );
    assert!(
        equirect::dir_to_uv(Vec3::Y)[1].abs() < 1e-6,
        "v = 0 straight up"
    );
    assert!(equirect::dir_to_uv(Vec3::X)[0] > 0.5, "u grows toward +X");
    for d in [
        Vec3::new(0.3, 0.4, -0.866),
        Vec3::new(-0.7, -0.1, 0.7),
        Vec3::new(0.0, -0.9, 0.43),
    ] {
        let d = d.normalize();
        let back = equirect::uv_to_dir(equirect::dir_to_uv(d));
        assert!(back.distance(d) < 1e-5, "{d} → {back}");
    }
}

/// `shadows` on point and spot lights (M4 4.7c): `None` unless written, so existing scenes load
/// unchanged; settings round-trip.
#[test]
fn local_shadow_settings_default_to_none_and_roundtrip() {
    use concordia_render_types::{LocalShadowSettings, PcfKernel, PointLight, SpotLight};
    assert_eq!(PointLight::default().shadows, None);
    assert_eq!(SpotLight::default().shadows, None);
    let old: PointLight = serde_json::from_value(serde_json::json!({
        "color": [1.0, 0.5, 0.2], "intensity_lumens": 800.0, "range": 6.0
    }))
    .unwrap();
    assert_eq!(old.shadows, None, "existing scenes load unchanged");
    let s = LocalShadowSettings::default();
    assert_eq!(
        (s.depth_bias, s.normal_bias, s.pcf),
        (1.0, 1.0, PcfKernel::X3)
    );
    let spot = SpotLight {
        shadows: Some(LocalShadowSettings {
            depth_bias: 2.0,
            ..s
        }),
        ..SpotLight::default()
    };
    let text = serde_json::to_value(&spot).unwrap();
    assert_eq!(serde_json::from_value::<SpotLight>(text).unwrap(), spot);
    let partial: LocalShadowSettings =
        serde_json::from_value(serde_json::json!({ "pcf": "X5" })).unwrap();
    assert_eq!((partial.pcf, partial.depth_bias), (PcfKernel::X5, 1.0));
}

/// Light sizes (M4 4.7d): zero unless written (today's hard lights), and they round-trip.
#[test]
fn light_sizes_default_to_zero_and_roundtrip() {
    use concordia_render_types::{DirectionalLight, PointLight, SpotLight};
    assert_eq!(DirectionalLight::default().angular_radius, 0.0);
    assert_eq!(PointLight::default().source_radius, 0.0);
    assert_eq!(SpotLight::default().source_radius, 0.0);
    let old: DirectionalLight =
        serde_json::from_value(serde_json::json!({ "illuminance_lux": 1000.0 })).unwrap();
    assert_eq!(old.angular_radius, 0.0, "existing scenes load unchanged");
    let sun = DirectionalLight {
        angular_radius: 0.0047,
        ..DirectionalLight::default()
    };
    let text = serde_json::to_value(&sun).unwrap();
    assert_eq!(
        serde_json::from_value::<DirectionalLight>(text).unwrap(),
        sun
    );
    let point = PointLight {
        source_radius: 0.1,
        ..PointLight::default()
    };
    let text = serde_json::to_value(&point).unwrap();
    assert_eq!(serde_json::from_value::<PointLight>(text).unwrap(), point);
}
