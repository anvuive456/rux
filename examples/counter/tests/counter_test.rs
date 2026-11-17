use counter::CounterApp;
use rux::testing::Harness;

#[test]
fn starts_at_zero() {
    let h = Harness::new(CounterApp::new());
    assert!(h.dump().contains(r#"text="Count: 0""#));
}

#[test]
fn increments_and_decrements() {
    let mut h = Harness::new(CounterApp::new());
    h.click_text("+");
    h.click_text("+");
    h.click_text("-");
    assert!(h.dump().contains(r#"text="Count: 1""#));
}

#[test]
fn click_only_patches_count_label() {
    let mut h = Harness::new(CounterApp::new());
    let before = h.renderer.applied;
    h.click_text("+");
    assert_eq!(
        h.renderer.applied - before,
        1,
        "only the count label should change"
    );
}
