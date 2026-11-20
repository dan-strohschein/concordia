# wanderers

Sample gameplay module (M2 phase 2.6d): wanderers turn at random (the processor's seeded stream), move with libm trigonometry, tire, raise a `wanderers.Tired` signal, rest until their energy recovers, and wander again. `concordia-scripting`'s tests run it as WASM, as a native library and statically linked, and require identical world hashes.
