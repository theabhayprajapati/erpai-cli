mod common;
use common::*;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn sends_bearer_and_user_agent_and_parses_json() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/app"))
        .and(query_param("pageSize", "1"))
        .and(header("authorization", "Bearer erp_pat_live_test"))
        .and(header("user-agent", "erpai-cli/0.2.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data":[{"_id":"a1"}],"totalCount":1})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let p = profile_for(&s, dir.path(), &[]);
    let c = erpai::client::ApiClient::new(&p).unwrap();
    let v = c
        .get("/v1/app-builder/app", &[("pageSize", "1")])
        .await
        .unwrap();
    assert_eq!(v["totalCount"], 1);
}

#[tokio::test]
async fn maps_statuses_to_error_codes() {
    let s = server().await;
    for status in [401u16, 404, 400, 409] {
        Mock::given(method("GET"))
            .and(path(format!("/v1/s{status}")))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(serde_json::json!({"message":"nope"}))
                    .insert_header("x-request-id", "req-1"),
            )
            .mount(&s)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/v1/s403"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "message":"API key not permitted to access this app","allowedApps":["a1","a2"]
        })))
        .mount(&s)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let c = erpai::client::ApiClient::new(&profile_for(&s, dir.path(), &[])).unwrap();
    for (status, code) in [
        (401, "auth_error"),
        (404, "not_found"),
        (400, "api_error"),
        (409, "api_error"),
    ] {
        let e = c.get(&format!("/v1/s{status}"), &[]).await.unwrap_err();
        assert_eq!(e.code.as_str(), code, "status {status}");
        assert_eq!(e.request_id.as_deref(), Some("req-1"));
    }
    let e = c.get("/v1/s403", &[]).await.unwrap_err();
    assert_eq!(e.code.as_str(), "forbidden");
    assert!(e.hint.unwrap().contains("a1, a2"));
}

#[tokio::test]
async fn retries_429_and_5xx_then_succeeds() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/flaky"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/flaky"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok":true})))
        .mount(&s)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let c = erpai::client::ApiClient::new(&profile_for(&s, dir.path(), &[])).unwrap();
    assert_eq!(c.get("/v1/flaky", &[]).await.unwrap()["ok"], true);
}

#[tokio::test]
async fn connection_refused_is_network_error() {
    let p = erpai::config::Profile::new("default", "http://127.0.0.1:9", "erp_pat_live_test");
    let c = erpai::client::ApiClient::new(&p).unwrap();
    assert_eq!(
        c.get("/v1/x", &[]).await.unwrap_err().code.as_str(),
        "network_error"
    );
}
