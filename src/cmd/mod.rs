pub mod apps;
pub mod attachments;
pub mod catalog;
pub mod columns;
pub mod documents;
pub mod forms;
pub mod layouts;
pub mod pages;
pub mod records;
pub mod roles;
pub mod settings;
pub mod sql;
pub mod tables;
pub mod widgets;
pub mod workflows;

use crate::cli::Global;
use crate::client::ApiClient;
use crate::config::{require_profile, Profile};
use crate::error::{CliError, Result};
use crate::output::{Context, Output, Page, Rendered};
use serde_json::Value;
use std::path::Path;

pub fn ctx(g: &Global) -> Result<(Profile, ApiClient)> {
    let p = require_profile(g)?;
    let c = ApiClient::new(&p)?;
    Ok((p, c))
}

pub fn context(profile: &Profile, app: Option<&str>) -> Context {
    Context {
        profile: profile.name.clone(),
        org: profile.org_name.clone().or(profile.org_id.clone()),
        app: app.map(str::to_string),
    }
}

pub fn page_args(page: u32, size: u32) -> Result<Vec<(String, String)>> {
    if page == 0 {
        return Err(CliError::validation("--page is 1-based"));
    }
    if !(1..=500).contains(&size) {
        return Err(CliError::validation("--page-size must be 1..=500"));
    }
    Ok(vec![
        ("pageNo".into(), page.to_string()),
        ("pageSize".into(), size.to_string()),
    ])
}

pub fn list_from(v: &Value, page: u32, size: u32) -> Rendered {
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = v
        .get("totalCount")
        .and_then(Value::as_u64)
        .unwrap_or(data.len() as u64);
    Output::list(
        data,
        Some(Page {
            no: page,
            size,
            total,
        }),
    )
}

pub fn item_from(v: Value) -> Rendered {
    let inner = if let Some(b) = v.get("body") {
        b.clone()
    } else if let Some(d) = v.get("data") {
        d.clone()
    } else {
        v
    };
    Output::item(inner)
}

pub fn read_json_arg(inline: Option<&str>, file: Option<&Path>) -> Result<Value> {
    match (inline, file) {
        (Some(s), None) => Ok(serde_json::from_str(s)?),
        (None, Some(f)) if f == Path::new("-") => {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
            Ok(serde_json::from_str(&s)?)
        }
        (None, Some(f)) => Ok(serde_json::from_slice(&std::fs::read(f)?)?),
        (None, None) => Err(CliError::validation("a JSON body is required")
            .with_hint("pass --body '<json>' or --file <path> (or --file - for stdin)")),
        (Some(_), Some(_)) => Err(CliError::validation(
            "pass either --body or --file, not both",
        )),
    }
}

pub fn q(pairs: &[(String, String)]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

pub fn dry_run_plan(method: &str, path: &str, body: Option<&Value>, ctx: Context) -> Rendered {
    Output::item(serde_json::json!({
        "dryRun": true,
        "request": { "method": method, "path": path, "body": body }
    }))
    .with_context(ctx)
}
