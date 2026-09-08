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
    /// Extra request header as "Name: value" (repeatable), e.g. the contract precondition headers.
    /// Authorization, cookie and gateway identity headers are never overridable.
    #[arg(long = "header", value_name = "NAME: VALUE")]
    pub header: Vec<String>,
}

const RESERVED_HEADERS: [&str; 6] = [
    "authorization",
    "cookie",
    "host",
    "content-length",
    "accept",
    "content-type",
];

fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>> {
    let mut out = Vec::with_capacity(raw.len());
    for h in raw {
        let (k, v) = h
            .split_once(':')
            .ok_or_else(|| CliError::validation(format!("--header '{h}' must be 'Name: value'")))?;
        let name = k.trim().to_ascii_lowercase();
        if name.is_empty()
            || RESERVED_HEADERS.contains(&name.as_str())
            || name.starts_with("x-gateway-")
        {
            return Err(CliError::validation(format!(
                "--header '{}' is not overridable",
                k.trim()
            )));
        }
        out.push((name, v.trim().to_string()));
    }
    Ok(out)
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
    let headers = parse_headers(&c.header)?;
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
    let http_method = match c.method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Patch => reqwest::Method::PATCH,
        Method::Delete => reqwest::Method::DELETE,
    };
    match c.method {
        Method::Get => Ok(render(
            api.request(http_method, &c.path, &qv, None, &headers)
                .await?,
        )
        .with_context(cx)),
        Method::Delete => match gate.confirm(&format!("DELETE {}", c.path), &cx)? {
            Decision::DryRun => Ok(dry_run_plan("DELETE", &c.path, body.as_ref(), cx)),
            Decision::Proceed => Ok(render(
                api.request(http_method, &c.path, &qv, body.as_ref(), &headers)
                    .await?,
            )
            .with_context(cx)),
        },
        Method::Post | Method::Put | Method::Patch => {
            let b = body.unwrap_or_else(|| serde_json::json!({}));
            if gate.dry_run {
                return Ok(dry_run_plan(method_name, &c.path, Some(&b), cx));
            }
            Ok(render(
                api.request(http_method, &c.path, &qv, Some(&b), &headers)
                    .await?,
            )
            .with_context(cx))
        }
    }
}
