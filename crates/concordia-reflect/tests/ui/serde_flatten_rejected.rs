use concordia_reflect::Reflect;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize, Reflect)]
struct Inner {
    a: u8,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize, Reflect)]
struct Outer {
    #[serde(flatten)]
    inner: Inner,
}

fn main() {}
