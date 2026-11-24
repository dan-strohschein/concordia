//! Counter-based sampling: deterministic per (seed, pixel, sample), uniform, decorrelated.

use concordia_pathtrace::SampleStream;

#[test]
fn streams_are_deterministic_and_in_the_unit_interval() {
    let draw = |seed, pixel, sample| {
        let mut s = SampleStream::new(seed, pixel, sample);
        (0..8).map(|_| s.next_f32()).collect::<Vec<_>>()
    };
    assert_eq!(draw(1, 2, 3), draw(1, 2, 3), "same inputs, same numbers");
    assert_ne!(draw(1, 2, 3), draw(1, 2, 4), "another sample");
    assert_ne!(draw(1, 2, 3), draw(1, 3, 3), "another pixel");
    assert_ne!(draw(1, 2, 3), draw(2, 2, 3), "another seed");
    for v in draw(9, 9, 9) {
        assert!((0.0..1.0).contains(&v), "{v}");
    }
}

#[test]
fn values_are_uniform_and_dimensions_decorrelated() {
    let n = 1 << 16;
    let (mut sum, mut sum_xy) = (0.0f64, 0.0f64);
    let mut buckets = [0u32; 16];
    for i in 0..n {
        let mut s = SampleStream::new(42, i, 0);
        let (x, y) = (f64::from(s.next_f32()), f64::from(s.next_f32()));
        sum += x;
        sum_xy += (x - 0.5) * (y - 0.5);
        buckets[(x * 16.0) as usize] += 1;
    }
    let mean = sum / f64::from(n);
    assert!((mean - 0.5).abs() < 0.005, "mean {mean}");
    let cov = sum_xy / f64::from(n);
    assert!(
        cov.abs() < 0.002,
        "covariance of consecutive dimensions {cov}"
    );
    let expected = n / 16;
    for (i, b) in buckets.iter().enumerate() {
        assert!(
            b.abs_diff(expected) < expected / 10,
            "bucket {i}: {b} vs {expected}"
        );
    }
}
