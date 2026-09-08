mod common;
use common::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn wrong_app_never_sends_a_request() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let cases: Vec<Vec<&str>> = vec![
        vec!["records", "query", "t1"],
        vec!["records", "delete", "t1", "r1", "--yes"],
        vec!["tables", "delete", "t1", "--yes"],
        vec!["columns", "list", "t1"],
        vec!["sql", "run", "--query", "select 1"],
    ];
    for args in cases {
        let mut full = args.clone();
        full.extend(["--app", "a2"]);
        let a = erpai(d.path()).args(&full).assert().code(4);
        assert_eq!(stderr_json(&a)["error"]["code"], "forbidden", "{args:?}");
    }
    assert_eq!(s.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn empty_filter_writes_are_refused_before_any_request() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    for cmd in ["delete-by-filter", "update-by-filter"] {
        let mut args = vec![
            "records",
            cmd,
            "--app",
            "a1",
            "t1",
            "--filter",
            r#"{"conditions":[]}"#,
            "--yes",
        ];
        if cmd == "update-by-filter" {
            args.extend(["--cells", r#"{"c1":1}"#]);
        }
        let a = erpai(d.path()).args(&args).assert().code(2);
        assert!(stderr_json(&a)["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("--all-rows"));
    }
    assert_eq!(s.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn dry_run_sends_no_mutating_request() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/app-builder/table/t1/record/count"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"count":1})))
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let f = r#"{"conditions":[{"colId":"c1","opr":"eq","value":1}]}"#;
    let cases: Vec<Vec<&str>> = vec![
        vec!["records", "create", "t1", "--cells", "{}"],
        vec!["records", "update", "t1", "r1", "--cells", "{}"],
        vec!["records", "delete", "t1", "r1"],
        vec!["records", "bulk-delete", "t1", "r1", "r2"],
        vec![
            "records",
            "update-by-filter",
            "t1",
            "--filter",
            f,
            "--cells",
            "{}",
        ],
        vec!["records", "delete-by-filter", "t1", "--filter", f],
        vec!["tables", "create", "--name", "X"],
        vec!["tables", "delete", "t1"],
        vec!["columns", "add", "t1", "--body", "{}"],
        vec!["columns", "delete", "t1", "c1"],
    ];
    for c in cases {
        let mut full = c.clone();
        full.extend(["--app", "a1", "--dry-run"]);
        let v = stdout_json(&erpai(d.path()).args(&full).assert().success());
        assert_eq!(v["data"]["dryRun"], true, "{c:?}");
    }
    let reqs = s.received_requests().await.unwrap();
    assert!(
        reqs.iter()
            .all(|r| r.method == "POST" && r.url.path().ends_with("/record/count")),
        "only count previews allowed: {:?}",
        reqs.iter()
            .map(|r| (r.method.to_string(), r.url.path().to_string()))
            .collect::<Vec<_>>()
    );
}
