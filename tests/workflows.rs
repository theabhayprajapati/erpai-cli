mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn list_and_patch_node() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/auto-builder/workflows"))
        .and(query_param("appId", "a1"))
        .and(query_param("pageSize", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"data":[{"_id":"w1","name":"WF","active":true}],"totalCount":1}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/v1/auto-builder/workflows/w1/nodes/n1"))
        .and(body_json(
            serde_json::json!({"set":{"parameters.customActionName":"aB3x"}}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"data":{"_id":"w1","rev":4}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args(["workflows", "list", "--app", "a1"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"][0]["name"], "WF");
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "workflows",
                "patch-node",
                "--app",
                "a1",
                "w1",
                "n1",
                "--set",
                r#"{"parameters.customActionName":"aB3x"}"#,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["rev"], 4);
}

#[tokio::test]
async fn create_requires_a_graph_and_delete_is_gated() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let a = erpai(d.path())
        .args([
            "workflows",
            "create",
            "--app",
            "a1",
            "--body",
            r#"{"name":"x"}"#,
        ])
        .assert()
        .code(2);
    assert!(stderr_json(&a)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("nodes"));
    erpai(d.path())
        .args(["workflows", "delete", "--app", "a1", "w1"])
        .assert()
        .code(2);
    assert_eq!(s.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn execute_dry_run_sends_nothing_and_options_posts_body() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path(
            "/v1/auto-builder/nodes/appEventTrigger/parameters/customActionName/options",
        ))
        .and(body_json(serde_json::json!({"tableId":"t1","appId":"a1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"success":true,"data":[{"name":"Convert","value":"aB3x"}]}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args(["workflows", "execute", "--app", "a1", "w1", "--dry-run"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["dryRun"], true);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "workflows",
                "nodes",
                "options",
                "--app",
                "a1",
                "appEventTrigger",
                "customActionName",
                "--body",
                r#"{"tableId":"t1"}"#,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"][0]["value"], "aB3x");
    let reqs = s.received_requests().await.unwrap();
    assert!(reqs.iter().all(|r| r.url.path().ends_with("/options")));
}

#[tokio::test]
async fn credentials_delete_and_test() {
    let s = server().await;
    Mock::given(method("DELETE"))
        .and(path("/v1/auto-builder/credentials/c1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"success":true})))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/auto-builder/credentials/c1/test"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"data":{"ok":true}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    erpai(d.path())
        .args(["workflows", "credentials", "delete", "--app", "a1", "c1"])
        .assert()
        .code(2);
    erpai(d.path())
        .args([
            "workflows",
            "credentials",
            "delete",
            "--app",
            "a1",
            "c1",
            "--yes",
        ])
        .assert()
        .success();
    let v = stdout_json(
        &erpai(d.path())
            .args(["workflows", "credentials", "test", "--app", "a1", "c1"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["ok"], true);
}

#[tokio::test]
async fn execute_and_test_node_wrap_the_input_for_the_engine() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/auto-builder/workflows/w1/execute"))
        .and(body_json(
            serde_json::json!({"triggerData":{"recordId":"r1"}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"success":true,"data":{"id":"e1","status":"success"}}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/auto-builder/workflows/w1/nodes/n1/test-execute"))
        .and(body_json(serde_json::json!({"testData":{"amount":5}})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"success":true,"outputData":[{"json":{"ok":true}}]}),
            ),
        )
        .expect(2)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "workflows",
                "execute",
                "--app",
                "a1",
                "w1",
                "--input",
                r#"{"recordId":"r1"}"#,
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["id"], "e1");
    // a bare item is wrapped; a body that already carries testData is sent verbatim
    for input in [r#"{"amount":5}"#, r#"{"testData":{"amount":5}}"#] {
        let v = stdout_json(
            &erpai(d.path())
                .args([
                    "workflows",
                    "test-node",
                    "--app",
                    "a1",
                    "w1",
                    "n1",
                    "--input",
                    input,
                ])
                .assert()
                .success(),
        );
        assert_eq!(v["data"]["outputData"][0]["json"]["ok"], true);
    }
}
