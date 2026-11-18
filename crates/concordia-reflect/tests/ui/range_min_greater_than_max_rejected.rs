use concordia_reflect::Reflect;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize, Reflect)]
struct Health {
    #[reflect(range(min = 10.0, max = 1.0))]
    hp: f32,
}

fn main() {}
