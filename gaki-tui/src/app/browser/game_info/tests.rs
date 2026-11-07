use super::*;

#[test]
fn tabs_cycle_both_ways() {
    assert_eq!(InfoTab::Details.step(true), InfoTab::Checks);
    assert_eq!(InfoTab::Artwork.step(true), InfoTab::Details);
    assert_eq!(InfoTab::Details.step(false), InfoTab::Artwork);
}
