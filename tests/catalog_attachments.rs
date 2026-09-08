mod common;
use common::*;
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn catalog_publish_injects_app_and_unpublish_gated() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/template-catalog/publish"))
        .and(body_string_contains("\"appId\":\"a1\""))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"body":{"slug":"crm"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "catalog",
                "publish",
                "--app",
                "a1",
                "--yes",
                "--body",
                r#"{"slug":"crm","title":"CRM"}"#,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["slug"], "crm");
    erpai(d.path())
        .args(["catalog", "unpublish", "--app", "a1", "crm"])
        .assert()
        .code(2);
    // publish is confirmed too: without --yes off a terminal nothing is sent
    erpai(d.path())
        .args([
            "catalog",
            "publish",
            "--app",
            "a1",
            "--body",
            r#"{"slug":"crm"}"#,
        ])
        .assert()
        .code(2);
}

#[tokio::test]
async fn catalog_activate_and_get_are_org_level_and_publisher_aware() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/template-catalog/publishers/acme/crm"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"success":true,"data":{"slug":"crm","status":"published"}}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/template-catalog/crm/activate"))
        .and(body_string_contains("\"name\":\"Sales\""))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "success":true,
            "data":{"runtimeAppId":"r1","activation":{"id":"act1","status":"active"},"reused":false}
        })))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/v1/app-builder/template-catalog/publisher/terms/accept",
        ))
        .and(body_string_contains("\"contentDigest\":\"d1\""))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"data":{"accepted":true}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/v1/app-builder/template-catalog/source-app/a1/publication",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"data":{"slug":"crm"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args(["catalog", "get", "crm", "--publisher", "acme"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["status"], "published");
    // activation is confirmed: refused off a terminal without --yes, no request sent
    erpai(d.path())
        .args(["catalog", "activate", "crm"])
        .assert()
        .code(2);
    let v = stdout_json(
        &erpai(d.path())
            .args(["catalog", "activate", "crm", "--name", "Sales", "--yes"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["runtimeAppId"], "r1");
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "catalog",
                "publisher",
                "accept-terms",
                "--version",
                "3",
                "--content-digest",
                "d1",
                "--yes",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["accepted"], true);
    let v = stdout_json(
        &erpai(d.path())
            .args(["catalog", "source-publication", "--app", "a1"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["slug"], "crm");
}

#[tokio::test]
async fn attachments_upload_is_multipart_and_builds_a_cell_value() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/attachment/file-upload"))
        .and(query_param("appId", "a1"))
        .and(body_string_contains("name=\"file-entity\""))
        .and(body_string_contains("APP_BUILDER_ATTACHMENT_RECORD"))
        .and(body_string_contains("filename=\"invoice.pdf\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"relativePath":"app-builder/attachments/x/invoice.pdf"}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let f = d.path().join("invoice.pdf");
    std::fs::write(&f, b"%PDF-1.4 test").unwrap();
    let v = stdout_json(
        &erpai(d.path())
            .args(["attachments", "upload", "--app", "a1", f.to_str().unwrap()])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["attachment"]["extension"], "pdf");
    assert_eq!(v["data"]["attachment"]["type"], "application/pdf");
    assert_eq!(v["data"]["attachment"]["isProtected"], true);
    assert_eq!(v["data"]["path"], "app-builder/attachments/x/invoice.pdf");
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "attachments",
                "upload",
                "--app",
                "a1",
                f.to_str().unwrap(),
                "--dry-run",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["dryRun"], true);
}
