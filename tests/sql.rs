mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn sql_schema_and_run() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/agent/app/sql/tables"))
        .and(query_param("appId", "a1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!([{"tableName":"v_a1_orders","columns":[{"columnName":"amount","dataType":"Float64"}]}]),
        ))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/agent/app/sql/execute"))
        .and(body_json(serde_json::json!({"appId":"a1","sqlQuery":"SELECT count() AS n FROM v_a1_orders","limit":100})))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"success":true,"status":200,"data":{"rows":[{"n":3}],"fields":[{"name":"n","type":"UInt64"}],"rowCount":1}}),
        ))
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let v = stdout_json(
        &erpai(d.path())
            .args(["sql", "schema", "--app", "a1"])
            .assert()
            .success(),
    );
    assert_eq!(v["data"][0]["tableName"], "v_a1_orders");
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "sql",
                "run",
                "--app",
                "a1",
                "--query",
                "SELECT count() AS n FROM v_a1_orders",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["rows"][0]["n"], 3);
}

#[tokio::test]
async fn sql_run_refuses_mutations_client_side() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let a = erpai(d.path())
        .args(["sql", "run", "--app", "a1", "--query", "DROP TABLE x"])
        .assert()
        .code(2);
    assert_eq!(stderr_json(&a)["error"]["code"], "validation_error");
    assert_eq!(s.received_requests().await.unwrap().len(), 0);
}
