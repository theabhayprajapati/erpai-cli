mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn api_get_injects_app_and_unwraps() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/table/t1/count"))
        .and(query_param("appId", "a1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"body":{"count":3}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "api",
                "get",
                "/v1/app-builder/table/t1/count",
                "--app",
                "a1",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["count"], 3);
}

#[tokio::test]
async fn api_rejects_non_public_paths_and_wrong_app() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let a = erpai(d.path())
        .args(["api", "get", "/internal/v1/x"])
        .assert()
        .code(2);
    assert_eq!(stderr_json(&a)["error"]["code"], "validation_error");
    erpai(d.path())
        .args(["api", "get", "/v1/app-builder/app", "--app", "a2"])
        .assert()
        .code(4);
    erpai(d.path())
        .args(["api", "get", "/v1/../etc"])
        .assert()
        .code(2);
    assert_eq!(s.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn api_post_dry_run_and_delete_gated() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/evaluate/c1"))
        .and(body_json(serde_json::json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true})))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "api",
                "post",
                "/v1/app-builder/table/t1/evaluate/c1",
                "--app",
                "a1",
                "--dry-run",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["dryRun"], true);
    erpai(d.path())
        .args([
            "api",
            "post",
            "/v1/app-builder/table/t1/evaluate/c1",
            "--app",
            "a1",
        ])
        .assert()
        .success();
    erpai(d.path())
        .args(["api", "delete", "/v1/app-builder/table/t1", "--app", "a1"])
        .assert()
        .code(2);
    assert_eq!(
        s.received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count(),
        0
    );
}

#[tokio::test]
async fn api_forwards_extra_headers_but_never_reserved_ones() {
    let s = server().await;
    Mock::given(method("PUT"))
        .and(path(
            "/v1/app-builder/command-centre/a1/external-access/vendor-portal",
        ))
        .and(wiremock::matchers::header("x-erpai-contract-version", "7"))
        .and(wiremock::matchers::header("x-erpai-contract-digest", "abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"success":true,"data":{"slug":"vendor-portal","version":8}}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "api",
                "put",
                "/v1/app-builder/command-centre/a1/external-access/vendor-portal",
                "--app",
                "a1",
                "--body",
                r#"{"slug":"vendor-portal"}"#,
                "--header",
                "X-ERPAI-Contract-Version: 7",
                "--header",
                "X-ERPAI-Contract-Digest: abc",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["version"], 8);
    for bad in [
        "Authorization: Bearer x",
        "x-gateway-user-id: u1",
        "no-colon",
    ] {
        erpai(d.path())
            .args([
                "api",
                "get",
                "/v1/app-builder/app",
                "--app",
                "a1",
                "--header",
                bad,
            ])
            .assert()
            .code(2);
    }
    assert_eq!(s.received_requests().await.unwrap().len(), 1);
}
