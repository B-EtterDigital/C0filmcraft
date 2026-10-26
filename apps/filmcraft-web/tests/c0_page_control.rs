#![cfg(feature = "page-control")]
#[path = "../src/page_control/protocol.rs"]
mod page_control;
use serde_json::json;

#[test]
fn native_page_contract_rejects_hostile_envelopes() {
    for input in ["no", "[]", "{}", r#"{"method":4}"#, r#"{"method":"c0.inspect","params":4}"#] {
        assert!(page_control::parse(input).is_err());
    }
    assert_eq!(page_control::parse(r#"{"method":"c0.inspect","params":null}"#).unwrap(), ("c0.inspect".into(), json!({})));
}
#[test]
fn request_limits_and_input_counts_are_bounded() {
    let large = json!({"method":"ui.type","params":{"text":"x".repeat(page_control::MAX_REQUEST_BYTES)}}).to_string();
    assert!(page_control::parse(&large).is_err());
    assert!(page_control::parse(&large.replace("ui.type", "c0.openFile")).is_ok());
    for (method, key, limit) in [("ui.click", "count", page_control::MAX_CLICK_COUNT), ("ui.drag", "steps", page_control::MAX_DRAG_STEPS)] {
        assert!(page_control::parse(&json!({"method":method,"params":{key:limit}}).to_string()).is_ok());
        for value in [json!(limit + 1), json!(-1), json!("huge"), json!(u64::MAX)] {
            assert!(page_control::parse(&json!({"method":method,"params":{key:value}}).to_string()).is_err());
        }
    }
}

#[test]
fn ui_requests_get_a_lent_ui_pass_and_settle_inside_the_call() {
    // The host start chain: c0.inspect -> c0.set {panels} -> ui.set {theme} -> c0.set {themeTokens}.
    for method in ["ui.set", "ui.render", "ui.elements", "ui.panel.show", "ui.click"] {
        assert!(page_control::needs_ui_pass(method), "{method}");
        assert!(page_control::inline_passes(method) >= 2, "{method}: answer pass plus reply-poll pass");
    }
    for method in ["c0.inspect", "c0.set", "ui.screenshot", "engine.execute"] {
        assert!(!page_control::needs_ui_pass(method), "{method}");
    }
    assert_eq!(page_control::inline_passes("c0.set"), 0);
}
