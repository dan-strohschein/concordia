//! `#[reflect(signal)]` marks a type as a signal (M4 4.9: the graph node library offers
//! `raise.<S>` and `on.<S>` for signals, `get`/`set` for other components).

use concordia_reflect::{Reflect, SignalMarker, TypeRegistry};
use serde::{Deserialize, Serialize};

/// Signal: something happened.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal)]
struct Happened {
    amount: f32,
}

/// Plain state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
struct Health {
    value: f32,
}

#[test]
fn signal_attribute_marks_the_type() {
    assert!(Happened::type_info().signal);
    assert!(!Health::type_info().signal);
}

#[test]
fn a_registration_can_be_marked_a_signal_at_run_time() {
    let mut reg = TypeRegistry::new();
    reg.register::<Health>().unwrap();
    let r = reg.get_by_name("Health").unwrap();
    assert!(!concordia_reflect::is_signal(r));
    reg.get_mut::<Health>().unwrap().insert_data(SignalMarker);
    assert!(concordia_reflect::is_signal(
        reg.get_by_name("Health").unwrap()
    ));
    reg.register::<Happened>().unwrap();
    assert!(concordia_reflect::is_signal(
        reg.get_by_name("Happened").unwrap()
    ));
}
