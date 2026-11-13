use perfhawk_server::serve;
use perfhawk_store::Store;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn responses_carry_hardening_headers() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let (addr, _h) = serve(store, 0).await.unwrap();
    for path in ["/", "/api/profiles", "/api/nope"] {
        let r = reqwest::get(format!("http://{addr}{path}")).await.unwrap();
        let h = r.headers();
        assert_eq!(h["x-frame-options"], "DENY", "{path}");
        assert_eq!(h["x-content-type-options"], "nosniff", "{path}");
        assert_eq!(h["referrer-policy"], "no-referrer", "{path}");
        let csp = h["content-security-policy"].to_str().unwrap();
        assert!(csp.contains("frame-ancestors 'none'"), "{path}: {csp}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_host_is_still_rejected_with_headers() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let (addr, _h) = serve(store, 0).await.unwrap();
    let c = reqwest::Client::new();
    let r = c
        .get(format!("http://{addr}/api/profiles"))
        .header("host", "evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 403);
    assert_eq!(r.headers()["x-frame-options"], "DENY");
}
