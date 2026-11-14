mod common;
use common::{delete, get, post_record, start, wait_finished};
use serde_json::{json, Value};
use std::net::SocketAddr;

async fn session_status(addr: SocketAddr, id: &str) -> Value {
    let (st, v) = get(addr, &format!("/api/sessions/{id}")).await;
    assert_eq!(st, 200, "{v}");
    v["status"].clone()
}

async fn cancel_current(addr: SocketAddr) {
    let (_, v) = get(addr, "/api/record/current").await;
    let id = v["job_id"].as_str().unwrap().to_string();
    assert_eq!(delete(addr, &format!("/api/record/{id}"), true).await, 204);
    wait_finished(addr).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn current_is_204_before_any_job() {
    let (_d, _s, addr) = start().await;
    let r = reqwest::get(format!("http://{addr}/api/record/current"))
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
}

#[tokio::test(flavor = "multi_thread")]
async fn record_runs_and_lands_a_session() {
    let (_d, _s, addr) = start().await;
    let (st, v) = post_record(addr, json!({"pid": 1, "duration_secs": 1}), true).await;
    assert_eq!(st, 202, "{v}");
    assert!(v["job_id"].is_string());
    let done = wait_finished(addr).await;
    assert_eq!(done["state"], "done", "{done}");
    assert!(done["progress"][0].as_str().unwrap().contains("capturing"));
    let id = done["session_id"].as_str().unwrap();
    assert_eq!(session_status(addr, id).await, "complete");
}

#[tokio::test(flavor = "multi_thread")]
async fn second_start_is_409_and_first_keeps_running() {
    let (_d, _s, addr) = start().await;
    let (st, first) = post_record(addr, json!({"pid": 2, "duration_secs": 60}), true).await;
    assert_eq!(st, 202, "{first}");
    let (st, v) = post_record(addr, json!({"pid": 1, "duration_secs": 1}), true).await;
    assert_eq!(st, 409, "{v}");
    assert_eq!(v["error"]["code"], "capture_running");
    let (st, cur) = get(addr, "/api/record/current").await;
    assert_eq!(st, 200);
    assert_eq!(cur["state"], "running");
    assert_eq!(cur["job_id"], first["job_id"]);
    cancel_current(addr).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_keeps_partial_and_state_cancelled() {
    let (_d, _s, addr) = start().await;
    post_record(addr, json!({"pid": 2, "duration_secs": 60}), true).await;
    cancel_current(addr).await;
    let (_, cur) = get(addr, "/api/record/current").await;
    assert_eq!(cur["state"], "cancelled", "{cur}");
    let id = cur["session_id"].as_str().expect("partial session kept");
    assert_eq!(session_status(addr, id).await, "cancelled");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_after_done_is_a_noop_and_unknown_id_is_404() {
    let (_d, _s, addr) = start().await;
    let (_, v) = post_record(addr, json!({"pid": 1, "duration_secs": 1}), true).await;
    wait_finished(addr).await;
    let id = v["job_id"].as_str().unwrap();
    assert_eq!(delete(addr, &format!("/api/record/{id}"), true).await, 204);
    let (_, cur) = get(addr, "/api/record/current").await;
    assert_eq!(cur["state"], "done");
    assert_eq!(delete(addr, "/api/record/nope", true).await, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_header_is_403_for_post_and_delete() {
    let (_d, _s, addr) = start().await;
    let (st, _) = post_record(addr, json!({"pid": 1, "duration_secs": 1}), false).await;
    assert_eq!(st, 403);
    assert_eq!(delete(addr, "/api/record/x", false).await, 403);
    let r = reqwest::get(format!("http://{addr}/api/record/current"))
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204, "no job was started");
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_targets_are_400() {
    let (_d, _s, addr) = start().await;
    for body in [
        json!({"pid": 1, "url": "http://127.0.0.1:6060", "duration_secs": 5}),
        json!({"duration_secs": 5}),
        json!({"url": "http://evil.com", "duration_secs": 5}),
        json!({"url": "http://127.0.0.1.evil.com", "duration_secs": 5}),
        json!({"url": "file:///etc/passwd", "duration_secs": 5}),
    ] {
        let (st, v) = post_record(addr, body.clone(), true).await;
        assert_eq!(st, 400, "{body} -> {v}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn duration_bounds() {
    let (_d, _s, addr) = start().await;
    for secs in [0, 601] {
        let (st, v) = post_record(addr, json!({"pid": 1, "duration_secs": secs}), true).await;
        assert_eq!(st, 400, "{secs}: {v}");
    }
    let (st, v) = post_record(addr, json!({"pid": 2, "duration_secs": 600}), true).await;
    assert_eq!(st, 202, "{v}");
    cancel_current(addr).await;
    let (st, v) = post_record(addr, json!({"pid": 1, "duration_secs": 1}), true).await;
    assert_eq!(st, 202, "{v}");
    wait_finished(addr).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_kind_is_400_with_the_valid_kinds() {
    let (_d, _s, addr) = start().await;
    let (st, v) = post_record(
        addr,
        json!({"pid": 1, "duration_secs": 5, "kinds": ["bogus"]}),
        true,
    )
    .await;
    assert_eq!(st, 400, "{v}");
    assert!(v["error"]["message"].as_str().unwrap().contains("block_io"));
}

#[tokio::test(flavor = "multi_thread")]
async fn unavailable_kind_fails_the_job_as_unsupported() {
    let (_d, _s, addr) = start().await;
    post_record(
        addr,
        json!({"pid": 1, "duration_secs": 5, "kinds": ["wall"]}),
        true,
    )
    .await;
    let done = wait_finished(addr).await;
    assert_eq!(done["state"], "failed", "{done}");
    assert_eq!(done["error"]["code"], "unsupported");
    assert!(done["error"]["message"]
        .as_str()
        .unwrap()
        .contains("wall is off"));
}

#[tokio::test(flavor = "multi_thread")]
async fn privilege_error_carries_the_rerun_command() {
    let (_d, _s, addr) = start().await;
    post_record(
        addr,
        json!({"pid": 3, "duration_secs": 5, "kinds": ["cpu"]}),
        true,
    )
    .await;
    let done = wait_finished(addr).await;
    assert_eq!(done["state"], "failed", "{done}");
    assert_eq!(done["error"]["code"], "needs_privilege");
    let m = done["error"]["message"].as_str().unwrap();
    assert!(m.contains("elevated shell"), "{m}");
    assert!(
        m.contains("perfhawk record --pid 3 --kinds cpu --duration 5s"),
        "{m}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn target_exiting_early_is_done_with_a_partial_session() {
    let (_d, _s, addr) = start().await;
    post_record(addr, json!({"pid": 4, "duration_secs": 5}), true).await;
    let done = wait_finished(addr).await;
    assert_eq!(done["state"], "done", "{done}");
    let id = done["session_id"].as_str().unwrap();
    assert_eq!(session_status(addr, id).await, "partial");
}
