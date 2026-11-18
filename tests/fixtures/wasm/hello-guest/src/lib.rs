//! Guest for the M-1 gate: greets, logs through the host, and traps on demand.

wit_bindgen::generate!({ world: "hello", path: "wit" });

struct Component;

impl Guest for Component {
    fn greet(name: String) -> String {
        concordia::fixtures::host::log(&format!("greet called with {name}"));
        format!("Hello, {name}!")
    }

    fn trap() {
        unreachable!("deliberate trap for the host containment test")
    }
}

export!(Component);
