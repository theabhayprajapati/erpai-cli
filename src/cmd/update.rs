use crate::cli::Global;
use crate::error::{CliError, Result};
use crate::output::{Output, Rendered};
use serde_json::{json, Value};

/// The marketplace repo carries both the plugin tree and the CLI binaries; CLI
/// releases are tagged `cli-v<version>` (plugin releases use `plugin-v*`).
const RELEASES_REPO: &str = "erphq/agent-plugins";
const CLI_TAG_PREFIX: &str = "cli-v";

#[derive(clap::Args, Debug)]
pub struct UpdateArgs {
    /// Only report; never changes anything (the plugin owns updates)
    #[arg(long)]
    pub check: bool,
}

fn managed_by_plugin() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .map(|p| p.contains("/erpai/bin/erpai-"))
        .unwrap_or(false)
}

fn newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|x| x.parse().ok())
            .collect()
    };
    parse(latest) > parse(current)
}

pub async fn run(_g: &Global, _a: UpdateArgs) -> Result<Rendered> {
    let api = std::env::var("ERPAI_UPDATE_API").unwrap_or_else(|_| "https://api.github.com".into());
    // `releases/latest` would answer with whichever tag is newest, plugin or CLI —
    // list instead and keep only the CLI ones.
    let url = format!(
        "{}/repos/{RELEASES_REPO}/releases?per_page=30",
        api.trim_end_matches('/')
    );
    let http = reqwest::Client::builder()
        .user_agent(format!("erpai-cli/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| CliError::internal(e.to_string()))?;
    let resp = http
        .get(&url)
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| CliError::network(format!("GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(CliError::network(format!(
            "release lookup failed: HTTP {}",
            resp.status().as_u16()
        )));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| CliError::network(e.to_string()))?;
    let releases: Vec<Value> = match &body {
        Value::Array(a) => a.clone(),
        other => vec![other.clone()],
    };
    let v = releases
        .into_iter()
        .filter(|r| {
            r.get("tag_name")
                .and_then(Value::as_str)
                .map(|t| t.starts_with(CLI_TAG_PREFIX))
                .unwrap_or(false)
                && r.get("draft") != Some(&Value::Bool(true))
        })
        .max_by_key(|r| {
            let t = r
                .get("tag_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_start_matches(CLI_TAG_PREFIX)
                .to_string();
            t.split('.')
                .filter_map(|x| x.parse::<u64>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or(Value::Null);
    let latest = v
        .get("tag_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_start_matches(CLI_TAG_PREFIX)
        .to_string();
    let current = env!("CARGO_PKG_VERSION");
    let available = !latest.is_empty() && newer(&latest, current);
    let managed = managed_by_plugin();
    let message = if !available {
        "you are on the latest version".to_string()
    } else if managed {
        "this binary is managed by the ERP AI agent plugin and pinned to a version — update the plugin (run the `erpai:update` skill) to move to the new CLI".to_string()
    } else {
        format!("a newer version is available — reinstall from https://github.com/{RELEASES_REPO}/releases/tag/{CLI_TAG_PREFIX}{latest}")
    };
    Ok(Output::item(json!({
        "current": current,
        "latest": if latest.is_empty() { Value::Null } else { latest.clone().into() },
        "updateAvailable": available,
        "managedByPlugin": managed,
        "releaseUrl": v.get("html_url").cloned().unwrap_or(Value::Null),
        "notes": v.get("body").cloned().unwrap_or(Value::Null),
        "message": message,
    })))
}

#[cfg(test)]
mod tests {
    use super::newer;
    #[test]
    fn semver_compare() {
        assert!(newer("0.3.0", "0.2.0"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.1.12", "0.2.0"));
    }
}
