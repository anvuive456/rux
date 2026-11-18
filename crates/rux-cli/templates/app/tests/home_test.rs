//! Headless UI tests: no emulator / simulator needed (`cargo test`).

use rux::testing::Harness;
use {{lib_name}}::{{project_name}}App;

#[test]
fn starts_at_zero() {
    let h = Harness::new({{project_name}}App::new());
    assert!(h.dump().contains(r#"text="Count: 0""#));
}

#[test]
fn increments_and_decrements() {
    let mut h = Harness::new({{project_name}}App::new());
    h.click_text("+");
    h.click_text("+");
    h.click_text("-");
    assert!(h.dump().contains(r#"text="Count: 1""#));
}
