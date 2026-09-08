#![allow(dead_code)]
use std::path::Path;
use wiremock::MockServer;

pub async fn server() -> MockServer {
    MockServer::start().await
}

pub fn profile_for(
    server: &MockServer,
    dir: &Path,
    allowed_apps: &[&str],
) -> erpai::config::Profile {
    let mut p = erpai::config::Profile::new("default", &server.uri(), "erp_pat_live_test");
    p.org_id = Some("org-1".into());
    p.org_name = Some("QA".into());
    p.allowed_apps = allowed_apps.iter().map(|s| s.to_string()).collect();
    erpai::config::ProfileStore::at(dir.to_path_buf())
        .save(&p)
        .unwrap();
    p
}

pub fn erpai(dir: &Path) -> assert_cmd::Command {
    let mut c = assert_cmd::Command::cargo_bin("erpai").unwrap();
    c.env("ERPAI_CONFIG_HOME", dir).env_remove("ERPAI_APP_ID");
    c
}

pub fn stdout_json(a: &assert_cmd::assert::Assert) -> serde_json::Value {
    serde_json::from_slice(&a.get_output().stdout).expect("stdout JSON")
}

pub fn stderr_json(a: &assert_cmd::assert::Assert) -> serde_json::Value {
    serde_json::from_slice(&a.get_output().stderr).expect("stderr JSON")
}
