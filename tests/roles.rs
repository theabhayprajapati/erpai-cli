mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn create_injects_module_ref_and_assign_is_gated() {
    let s = server().await;
    Mock::given(method("POST")).and(path("/v1/permission/role"))
        .and(body_json(serde_json::json!({"name":"Viewer","permissions":{"data":[{"subModule":"t1","permissions":["read"]}]},"moduleRef":"a1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true,"body":{"_id":"r1"}}))).expect(1).mount(&s).await;
    Mock::given(method("PATCH")).and(path("/v1/permission/user-role-mapping"))
        .and(body_json(serde_json::json!({"module":"APP","moduleRef":"a1","data":[{"iamUserId":"u1","roleIds":["r1"]}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true}))).expect(1).mount(&s).await;
    Mock::given(method("GET"))
        .and(path("/v1/permission/role"))
        .and(query_param("moduleRef", "a1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data":[{"_id":"r1","name":"Viewer"}]})),
        )
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(&erpai(d.path()).args(["roles", "create", "--app", "a1", "--body", r#"{"name":"Viewer","permissions":{"data":[{"subModule":"t1","permissions":["read"]}]}}"#]).assert().success());
    assert_eq!(v["data"]["_id"], "r1");
    let v = stdout_json(
        &erpai(d.path())
            .args(["roles", "list", "--app", "a1"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"][0]["name"], "Viewer");
    erpai(d.path())
        .args([
            "roles", "assign", "--app", "a1", "--user", "u1", "--role", "r1",
        ])
        .assert()
        .code(2);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "roles",
                "invite",
                "--app",
                "a1",
                "--email",
                "qa@example.com",
                "--role",
                "r1",
                "--dry-run",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["dryRun"], true);
    erpai(d.path())
        .args([
            "roles", "assign", "--app", "a1", "--user", "u1", "--role", "r1", "--yes",
        ])
        .assert()
        .success();
    let reqs = s.received_requests().await.unwrap();
    assert!(reqs.iter().all(|r| !r.url.path().contains("user-invite")));
}
