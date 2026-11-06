use super::marquee;

#[test]
fn names_that_fit_are_unchanged() {
    assert_eq!(marquee("Doom", 10, 7), "Doom"); // shorter than the column
    assert_eq!(marquee("Doom", 4, 3), "Doom"); // exactly fills the column
    assert_eq!(marquee("Doom", 0, 1), "Doom"); // degenerate zero budget
}

#[test]
fn overflowing_names_scroll_and_keep_the_column_width() {
    let name = "Counter-Strike"; // 14 chars, wider than the 6-col budget
    let budget = 6;

    let s0 = marquee(name, budget, 0);
    assert_eq!(
        s0.chars().count(),
        budget,
        "window is exactly the column wide"
    );
    assert_eq!(s0, "Counte");

    // Each tick shifts the window one column to the left.
    assert_eq!(marquee(name, budget, 1), "ounter");

    // It loops: after a full period (name + the gap) it returns to the start.
    let period = name.chars().count() + "   •   ".chars().count();
    assert_eq!(marquee(name, budget, period), s0);
}
