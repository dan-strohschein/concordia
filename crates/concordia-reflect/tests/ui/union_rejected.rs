use concordia_reflect::Reflect;

#[derive(Reflect)]
union Bits {
    f: f32,
    u: u32,
}

fn main() {}
