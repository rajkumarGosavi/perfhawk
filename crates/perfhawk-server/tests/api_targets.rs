mod common;
use common::{get, start};

#[tokio::test(flavor = "multi_thread")]
async fn targets_endpoint_returns_shape() {
    let (_d, _s, addr) = start().await;
    let (st, v) = get(addr, "/api/targets").await;
    assert_eq!(st, 200, "{v}");
    assert!(v["targets"].is_array(), "{v}");
    assert!(v.get("warning").is_some(), "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn capabilities_lists_available_and_unavailable_with_note() {
    let (_d, _s, addr) = start().await;
    let (st, v) = get(addr, "/api/capabilities?pid=4242").await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["runtime"], "fake");
    let kinds = v["kinds"].as_array().unwrap();
    let cpu = kinds.iter().find(|k| k["kind"] == "cpu").unwrap();
    assert_eq!(cpu["available"], true);
    let wall = kinds.iter().find(|k| k["kind"] == "wall").unwrap();
    assert_eq!(wall["available"], false);
    assert_eq!(wall["note"], "wall is off in the fake");
}

#[tokio::test(flavor = "multi_thread")]
async fn capabilities_requires_exactly_one_target() {
    let (_d, _s, addr) = start().await;
    let (st, _) = get(addr, "/api/capabilities").await;
    assert_eq!(st, 400);
    let (st, _) = get(
        addr,
        "/api/capabilities?pid=1&url=http%3A%2F%2F127.0.0.1%3A6060",
    )
    .await;
    assert_eq!(st, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn capabilities_rejects_non_loopback_url() {
    let (_d, _s, addr) = start().await;
    let (st, v) = get(addr, "/api/capabilities?url=http%3A%2F%2Fevil.com%2F").await;
    assert_eq!(st, 400, "{v}");
    assert_eq!(v["error"]["code"], "bad_request");
}

#[tokio::test(flavor = "multi_thread")]
async fn capabilities_for_unrecognised_target_is_not_detected() {
    let (_d, _s, addr) = start().await;
    let (st, v) = get(addr, "/api/capabilities?url=http%3A%2F%2F127.0.0.1%3A1").await;
    assert_eq!(st, 400, "{v}");
    assert_eq!(v["error"]["code"], "not_detected");
}
