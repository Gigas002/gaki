use super::*;

#[test]
fn playtime_shows_minutes_under_an_hour_and_hours_above() {
    assert_eq!(format_playtime(0), "0 min");
    assert_eq!(format_playtime(59), "59 min");
    assert_eq!(format_playtime(60), "1.0 h");
    assert_eq!(format_playtime(741), "12.3 h");
}
