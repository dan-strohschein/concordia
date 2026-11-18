use concordia_reflect::Reflect;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize, Reflect)]
struct Wrapper<T> {
    inner: T,
}

fn main() {}
