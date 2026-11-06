use super::*;

struct Item(String);
impl Named for Item {
    fn get_name(&self) -> String {
        self.0.clone()
    }
    fn is_valid(&self) -> bool {
        true
    }
}

#[test]
fn selected_out_of_range_does_not_panic() {
    // Select the last row, then narrow the live query so `activated()`
    // shrinks below the stored index. `selected()` must yield `None`
    // instead of panicking (the old `.expect()` would have aborted here).
    let mut list =
        StatefulList::with_items(vec![Item("alpha".to_string()), Item("beta".to_string())]);
    list.state.select(Some(1));
    list.query = "alpha".to_string(); // now only one item is activated
    assert!(list.selected().is_none());
}

#[test]
fn selected_in_range_returns_item() {
    let mut list = StatefulList::with_items(vec![Item("alpha".to_string())]);
    list.state.select(Some(0));
    assert_eq!(
        list.selected().map(|i| i.0.clone()),
        Some("alpha".to_string())
    );
}
