mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn query_posts_filter_and_pages() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/paged-record"))
        .and(query_param("appId", "a1"))
        .and(query_param("pageNo", "2"))
        .and(query_param("pageSize", "50"))
        .and(query_param("sortCol", "UTDT"))
        .and(query_param("sortDir", "1"))
        .and(body_json(serde_json::json!({"conditions":[{"colId":"c1","opr":"eq","value":[1]}],"logicalOperator":"and"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"data":[{"_id":"r1","cells":{"c1":[1]}}],"totalCount":51}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "records",
                "query",
                "--app",
                "a1",
                "t1",
                "--filter",
                r#"{"conditions":[{"colId":"c1","opr":"eq","value":[1]}],"logicalOperator":"and"}"#,
                "--page",
                "2",
                "--page-size",
                "50",
                "--sort-col",
                "UTDT",
                "--sort-dir",
                "1",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["page"]["total"], 51);
    assert_eq!(v["data"][0]["_id"], "r1");
}

#[tokio::test]
async fn count_and_create_and_update() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/record/count"))
        .and(body_json(serde_json::json!({"filter":{"conditions":[{"colId":"c1","opr":"nemp"}],"logicalOperator":"and"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"count":7})))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/record"))
        .and(body_json(serde_json::json!({"cells":{"c1":"x"}})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"body":{"_id":"r9"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("PUT"))
        .and(path("/v1/app-builder/table/t1/record/r9"))
        .and(body_json(serde_json::json!({"cells":{"c1":"y"}})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"body":{"_id":"r9"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "records",
                "count",
                "--app",
                "a1",
                "t1",
                "--filter",
                r#"{"conditions":[{"colId":"c1","opr":"nemp"}]}"#,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["count"], 7);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "records",
                "create",
                "--app",
                "a1",
                "t1",
                "--cells",
                r#"{"c1":"x"}"#,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["_id"], "r9");
    erpai(d.path())
        .args([
            "records",
            "update",
            "--app",
            "a1",
            "t1",
            "r9",
            "--cells",
            r#"{"c1":"y"}"#,
        ])
        .assert()
        .success();
}

#[tokio::test]
async fn delete_by_filter_previews_count_then_needs_yes() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/record/count"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"count":3})))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/record-bulk-delete-by-filter"))
        .and(body_json(serde_json::json!({"filter":{"conditions":[{"colId":"c1","opr":"eq","value":[3]}],"logicalOperator":"and"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true,"deleted":3})))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let f = r#"{"conditions":[{"colId":"c1","opr":"eq","value":[3]}],"logicalOperator":"and"}"#;
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "records",
                "delete-by-filter",
                "--app",
                "a1",
                "t1",
                "--filter",
                f,
                "--dry-run",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["matching"], 3);
    assert_eq!(v["data"]["dryRun"], true);
    let a = erpai(d.path())
        .args([
            "records",
            "delete-by-filter",
            "--app",
            "a1",
            "t1",
            "--filter",
            f,
        ])
        .assert()
        .code(2);
    assert!(stderr_json(&a)["error"]["message"]
        .as_str()
        .unwrap()
        .contains('3'));
    erpai(d.path())
        .args([
            "records",
            "delete-by-filter",
            "--app",
            "a1",
            "t1",
            "--filter",
            f,
            "--yes",
        ])
        .assert()
        .success();
    assert_eq!(
        s.received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path().ends_with("record-bulk-delete-by-filter"))
            .count(),
        1
    );
}
