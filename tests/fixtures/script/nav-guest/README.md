# nav-guest

A gameplay module (`navg`) for `concordia-app`'s `tests/nav_guest.rs` (M2 phase 2.5b.1). It shows that a WASM guest reaches engine types by name: `navg.go` writes each `NavAgent`'s target from its `navg.Goal`, and `navg.listen` records the `AnimEvent` and `NavArrived` signals it receives in `navg.Heard`. The engine types are mirrored field for field, as a game's scripts would. Built by `just wasm` as `nav_guest.wasm`.
