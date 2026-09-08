mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn layouts_validate_type_and_pass_kanban_through() {
    let s = server().await;
    let kanban = serde_json::json!({"tableId":"t1","name":"Board","viewType":"kanban","config":{"selectedColumn":"c1","boardColsToBeDisplayed":["1","2"],"selectedDisplayColumns":["c2"]}});
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/layout"))
        .and(query_param("appId", "a1"))
        .and(body_json({
            let mut k = kanban.clone();
            k["layoutType"] = "kanban".into();
            k
        }))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"body":{"_id":"l1"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let a = erpai(d.path())
        .args([
            "layouts",
            "create",
            "--app",
            "a1",
            "--body",
            r#"{"tableId":"t1","name":"x","viewType":"board","config":{}}"#,
        ])
        .assert()
        .code(2);
    assert!(stderr_json(&a)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("board"));
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "layouts",
                "create",
                "--app",
                "a1",
                "--body",
                &kanban.to_string(),
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["_id"], "l1");
    erpai(d.path())
        .args(["layouts", "delete", "--app", "a1", "l1"])
        .assert()
        .code(2);
}

#[tokio::test]
async fn forms_set_posts_and_delete_is_gated() {
    let s = server().await;
    Mock::given(method("POST")).and(path("/v1/app-builder/table/t1/entry-form")).and(query_param("appId", "a1"))
        .and(body_json(serde_json::json!({"title":"F","fields":[{"_id":"c1","title":"Name","type":"column_view","required":true,"index":0,"uiVisible":true,"readOnly":false}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true,"body":{"title":"F"}}))).expect(1).mount(&s).await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let body = r#"{"title":"F","fields":[{"_id":"c1","title":"Name","type":"column_view","required":true,"index":0,"uiVisible":true,"readOnly":false}]}"#;
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "forms", "set", "--app", "a1", "--table", "t1", "--body", body,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["title"], "F");
    let a = erpai(d.path())
        .args([
            "forms",
            "set",
            "--app",
            "a1",
            "--table",
            "t1",
            "--body",
            r#"{"title":"x"}"#,
        ])
        .assert()
        .code(2);
    assert!(stderr_json(&a)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("fields"));
    erpai(d.path())
        .args(["forms", "delete", "--app", "a1", "--table", "t1"])
        .assert()
        .code(2);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "forms",
                "delete",
                "--app",
                "a1",
                "--table",
                "t1",
                "--dry-run",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["dryRun"], true);
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
