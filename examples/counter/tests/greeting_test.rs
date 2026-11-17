use counter::CounterApp;
use rux::testing::Harness;

#[test]
fn greets_anonymous_by_default() {
    let h = Harness::new(CounterApp::new());
    assert!(h.dump().contains(r#"text="Hello!""#));
}

#[test]
fn typing_name_updates_greeting() {
    let mut h = Harness::new(CounterApp::new());
    h.type_text("Rux");
    assert!(h.dump().contains(r#"text="Hello, Rux!""#));
}
