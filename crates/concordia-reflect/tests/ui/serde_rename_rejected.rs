use concordia_reflect::Reflect;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize, Reflect)]
#[serde(rename_all = "camelCase")]
struct Health {
    hit_points: f32,
}

fn main() {}
