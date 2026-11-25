//! Per-target color write masks (M4 4.3b2): a pipeline names the color targets it writes.

use concordia_rhi::{ColorWrites, Format, RhiError};

#[test]
fn color_writes_name_the_targets_a_pipeline_writes() {
    let all = ColorWrites::ALL;
    assert!((0..8).all(|t| all.writes(t)));
    assert_eq!(ColorWrites::default(), all);
    let first = ColorWrites::only(&[0]).unwrap();
    assert!(first.writes(0) && !first.writes(1) && !first.writes(7));
    let some = ColorWrites::only(&[1, 3]).unwrap();
    assert_eq!(
        (0..8).filter(|t| some.writes(*t)).collect::<Vec<_>>(),
        [1, 3]
    );
    let none = ColorWrites::only(&[]).unwrap();
    assert!((0..8).all(|t| !none.writes(t)));
    assert!(matches!(
        ColorWrites::only(&[8]),
        Err(RhiError::Invalid { .. })
    ));
}

#[test]
fn rg16_float_is_four_bytes_of_color() {
    assert_eq!(Format::Rg16Float.bytes_per_pixel(), 4);
    assert!(!Format::Rg16Float.is_depth() && !Format::Rg16Float.is_srgb());
}
