mod common;
use common::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn pages_validate_slug_and_refuse_key_literals() {
    let s = server().await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let good = d.path().join("p.html");
    std::fs::write(&good, "<html><script>window.ERPAI.query()</script></html>").unwrap();
    let leaky = d.path().join("leak.html");
    std::fs::write(
        &leaky,
        "<script>fetch('/v1/x',{headers:{Authorization:'Bearer erp_pat_live_abc'}})</script>",
    )
    .unwrap();
    let a = erpai(d.path())
        .args([
            "pages",
            "create",
            "--app",
            "a1",
            "--name",
            "X",
            "--slug",
            "Bad_Slug",
            "--html-file",
            good.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    assert!(stderr_json(&a)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("slug"));
    let a = erpai(d.path())
        .args([
            "pages",
            "create",
            "--app",
            "a1",
            "--name",
            "X",
            "--slug",
            "ok-slug",
            "--html-file",
            leaky.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    assert!(stderr_json(&a)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("key"));
    assert_eq!(s.received_requests().await.unwrap().len(), 0);
    Mock::given(method("POST"))
        .and(path("/v1/agent/app/custom-pages"))
        .and(query_param("appId", "a1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"success":true,"data":{"_id":"p1","slug":"ok-slug"}}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "pages",
                "create",
                "--app",
                "a1",
                "--name",
                "X",
                "--slug",
                "ok-slug",
                "--html-file",
                good.to_str().unwrap(),
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["slug"], "ok-slug");
}

#[tokio::test]
async fn widgets_set_posts_and_delete_gated() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/agent/app/table-html-widgets"))
        .and(query_param("appId", "a1"))
        .and(body_json(
            serde_json::json!({"tableId":"t1","html":"<div class=\"widget-stat\">1</div>"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"body":{"_id":"w1"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &["a1"]);
    let f = d.path().join("w.html");
    std::fs::write(&f, "<div class=\"widget-stat\">1</div>").unwrap();
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "widgets",
                "set",
                "--app",
                "a1",
                "--table",
                "t1",
                "--html-file",
                f.to_str().unwrap(),
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["_id"], "w1");
    erpai(d.path())
        .args(["widgets", "delete", "--app", "a1", "w1"])
        .assert()
        .code(2);
}
