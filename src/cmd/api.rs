use super::*;
use crate::safety::{preflight_app, Decision, Gate};
use clap::{Args, ValueEnum};
use std::path::PathBuf;

/// Generic request for endpoints without a dedicated command. Same credential, envelope,
/// error mapping and safety rules as every other command; the HTTP contract is documented
/// in the `erpai-api` skill.
#[derive(Args, Debug)]
pub struct ApiCmd {
    /// HTTP method
    pub method: Method,
    /// Public API path, e.g. /v1/app-builder/table/<table-id>/evaluate/<column-id>
    pub path: String,
    /// Query parameter as key=value (repeatable). `appId` is added from --app when absent.
    #[arg(long = "query", value_name = "KEY=VALUE")]
    pub query: Vec<String>,
    #[arg(long)]
    pub body: Option<String>,
    #[arg(long)]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

pub async fn run(g: &Global, c: ApiCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    if !(c.path.starts_with("/v1/") || c.path.starts_with("/open/v1/")) {
        return Err(CliError::validation(
            "path must start with /v1/ or /open/v1/",
        ));
    }
    if c.path.contains("..") || c.path.contains('?') {
        return Err(CliError::validation(
            "path must not contain '..' or a query string (use --query)",
        ));
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for q in &c.query {
        let (k, v) = q
            .split_once('=')
            .ok_or_else(|| CliError::validation(format!("--query '{q}' must be key=value")))?;
        pairs.push((k.to_string(), v.to_string()));
    }
    let app = g
        .app
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(a) = &app {
        preflight_app(&p, a)?;
        if !pairs.iter().any(|(k, _)| k == "appId") {
            pairs.push(("appId".into(), a.clone()));
        }
    }
    let cx = context(&p, app.as_deref());
    let gate = Gate::from(g);
    let body = match (&c.body, &c.file) {
        (None, None) => None,
        _ => Some(read_json_arg(c.body.as_deref(), c.file.as_deref())?),
    };
    let qv = q(&pairs);
    let method_name = match c.method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Put => "PUT",
        Method::Patch => "PATCH",
        Method::Delete => "DELETE",
    };
    let render = |v: Value| -> Rendered {
        let x = inner(&v);
        if let Some(arr) = x.as_array() {
            Output::list(arr.clone(), None)
        } else {
            Output::item(x)
        }
    };
    match c.method {
        Method::Get => Ok(render(api.get(&c.path, &qv).await?).with_context(cx)),
        Method::Delete => match gate.confirm(&format!("DELETE {}", c.path), &cx)? {
            Decision::DryRun => Ok(dry_run_plan("DELETE", &c.path, body.as_ref(), cx)),
            Decision::Proceed => {
                Ok(render(api.delete(&c.path, &qv, body.as_ref()).await?).with_context(cx))
            }
        },
        Method::Post | Method::Put | Method::Patch => {
            let b = body.unwrap_or_else(|| serde_json::json!({}));
            if gate.dry_run {
                return Ok(dry_run_plan(method_name, &c.path, Some(&b), cx));
            }
            let v = match c.method {
                Method::Post => api.post(&c.path, &qv, &b).await?,
                Method::Put => api.put(&c.path, &qv, &b).await?,
                _ => api.patch(&c.path, &qv, &b).await?,
            };
            Ok(render(v).with_context(cx))
        }
    }
}
