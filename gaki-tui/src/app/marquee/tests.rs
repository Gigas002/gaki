use super::*;

#[test]
fn advances_one_step_per_interval() {
    let mut marquee = Marquee::default();
    assert_eq!(marquee.tick_at(Some(0), Duration::ZERO), 0);
    assert_eq!(marquee.tick_at(Some(0), Duration::from_millis(1000)), 4);
}

#[test]
fn restarts_when_the_highlight_moves() {
    let mut marquee = Marquee::default();
    marquee.tick_at(Some(0), Duration::ZERO);
    assert_eq!(marquee.tick_at(Some(1), Duration::from_millis(1000)), 0);
    assert_eq!(marquee.tick_at(Some(1), Duration::from_millis(1500)), 2);
}
