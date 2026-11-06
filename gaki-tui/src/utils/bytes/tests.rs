use super::convert;

#[test]
fn formats_decimal_units() {
    assert_eq!(convert(0.0), "0 B");
    assert_eq!(convert(512.0), "512 B");
    assert_eq!(convert(1500.0), "1.5 kB");
    assert_eq!(convert(1000.0), "1 kB");
    assert_eq!(convert(12_345_678_901.0), "12.35 GB");
    assert_eq!(convert(-2_000_000.0), "-2 MB");
}
