use super::cycle;

#[test]
fn cycle_walks_the_values_then_unsets() {
    let values = ["a", "b"];
    assert_eq!(cycle(None, &values), Some("a"));
    assert_eq!(cycle(Some("a"), &values), Some("b"));
    assert_eq!(cycle(Some("b"), &values), None);
    assert_eq!(
        cycle(Some("gone"), &values),
        Some("a"),
        "a stale value restarts"
    );
    assert_eq!(cycle::<&str>(None, &[]), None);
}
