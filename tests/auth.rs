mod common;
use common::*;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn login_with_api_key_stores_profile_and_whoami_falls_back() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/whoami"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/app"))
        .and(header("authorization", "Bearer erp_pat_live_abc"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[],"totalCount":0})),
        )
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    let v = stdout_json(
        &erpai(d.path())
            .args([
                "login",
                "--base-url",
                &s.uri(),
                "--api-key",
                "erp_pat_live_abc",
            ])
            .assert()
            .success(),
    );
    assert_eq!(v["data"]["apiKey"]["prefix"], "erp_pat_live");
    let v = stdout_json(&erpai(d.path()).arg("whoami").assert().success());
    assert_eq!(v["data"]["source"], "login");
}

#[tokio::test]
async fn whoami_uses_endpoint_when_present() {
    let s = server().await;
    Mock::given(method("GET"))
        .and(path("/v1/app-builder/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "userId":"u1","email":"qa@example.com","tenantId":"org-1","orgName":"QA","authMethod":"apikey",
            "apiKey":{"id":"k1","name":"erpai CLI","scopes":["records:read"],"allowedApps":["a1"]}
        })))
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    profile_for(&s, d.path(), &[]);
    let v = stdout_json(&erpai(d.path()).arg("whoami").assert().success());
    assert_eq!(v["data"]["source"], "server");
    assert_eq!(v["data"]["apiKey"]["allowedApps"][0], "a1");
    let p = erpai::config::ProfileStore::at(d.path().to_path_buf())
        .load("default")
        .unwrap()
        .unwrap();
    assert_eq!(p.allowed_apps, vec!["a1".to_string()]);
}

#[tokio::test]
async fn browser_login_exchanges_code_and_mints_scoped_key() {
    let s = server().await;
    Mock::given(method("POST"))
        .and(path("/v1/onboarding/desktop-auth/exchange"))
        .and(body_json(serde_json::json!({"code":"C0DE"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token":"sess","user":{"id":"u1","email":"qa@example.com"},"session":{"activeOrganizationId":"org-1"}
        })))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/onboarding/api-key"))
        .and(header("authorization", "Bearer sess"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id":"k1","name":"erpai CLI","key":"erp_pat_live_new","scopes":["records:read"],"resourceRestrictions":{"appIds":["a1"]}
        })))
        .expect(1)
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("erpai"))
        .env("ERPAI_CONFIG_HOME", d.path())
        .args([
            "login",
            "--base-url",
            &s.uri(),
            "--no-browser",
            "--app",
            "a1",
            "--read-only",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = std::io::BufReader::new(child.stderr.take().unwrap());
    let mut first = String::new();
    std::io::BufRead::read_line(&mut stderr, &mut first).unwrap();
    let url = first
        .trim()
        .strip_prefix("open: ")
        .expect("open: line")
        .to_string();
    let u = url::Url::parse(&url).unwrap();
    let port = u
        .query_pairs()
        .find(|(k, _)| k == "port")
        .unwrap()
        .1
        .to_string();
    let state = u
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    let cb = format!("http://127.0.0.1:{port}/callback?code=C0DE&state={state}&apps=a1");
    let resp = reqwest::get(&cb).await.unwrap();
    assert_eq!(resp.status(), 200);
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["apiKey"]["allowedApps"][0], "a1");
    let p = erpai::config::ProfileStore::at(d.path().to_path_buf())
        .load("default")
        .unwrap()
        .unwrap();
    assert_eq!(p.api_key, "erp_pat_live_new");
    assert_eq!(p.org_id.as_deref(), Some("org-1"));
    let req = s
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path().ends_with("/api-key"))
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert!(body["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s.as_str().unwrap().ends_with(":read")));
    assert_eq!(body["resourceRestrictions"]["appIds"][0], "a1");
}

#[tokio::test]
async fn logout_revokes_then_forgets_even_if_revoke_is_refused() {
    let s = server().await;
    Mock::given(method("DELETE"))
        .and(path("/v1/onboarding/api-key/k1"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&s)
        .await;
    let d = tempfile::tempdir().unwrap();
    let mut p = profile_for(&s, d.path(), &[]);
    p.key_id = Some("k1".into());
    erpai::config::ProfileStore::at(d.path().to_path_buf())
        .save(&p)
        .unwrap();
    let v = stdout_json(&erpai(d.path()).arg("logout").assert().success());
    assert_eq!(v["data"]["revoked"], false);
    assert!(erpai::config::ProfileStore::at(d.path().to_path_buf())
        .load("default")
        .unwrap()
        .is_none());
}

#[test]
fn elevated_scopes_are_refused() {
    let d = tempfile::tempdir().unwrap();
    let a = erpai(d.path())
        .args([
            "login",
            "--api-key",
            "erp_pat_live_x",
            "--scopes",
            "admin:*",
        ])
        .assert()
        .code(2);
    assert!(stderr_json(&a)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("admin:*"));
}
