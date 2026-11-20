# conformance-guest

The script conformance module (`conformance`), written with `concordia-script-guest`: one processor per behavior the conformance suite (`concordia-script`, feature `conformance`) checks on every backend. It builds as an `rlib` (the in-process test backend links it) and, with `just wasm`, as `conformance_guest.wasm` (plus `conformance_guest_v2.wasm`, built with the `v2` feature, which doubles `conf.integrate`'s speed for reload tests). `just native-fixtures` builds it as a native library (`dylib` feature) in three variants: `conformance_guest`, `conformance_guest_v2`, and `conformance_guest_layout2` (`Position` gains `z`, `Velocity` loses `y`, for migration tests).

References: the wit-bindgen README and the `wasip2` crate documentation.
