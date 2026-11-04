use super::*;

#[test]
fn country_normalization() {
    assert_eq!(normalize_country("de").as_deref(), Some("DE"));
    assert_eq!(normalize_country(" uk ").as_deref(), Some("GB"));
    assert_eq!(normalize_country("usa"), None);
    assert_eq!(normalize_country("u1"), None);
}
