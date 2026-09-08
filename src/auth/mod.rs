pub mod callback;

use crate::cli::Global;
use crate::client::ApiClient;
use crate::config::{require_profile, Profile, ProfileStore, DEFAULT_BASE_URL};
use crate::error::{CliError, ErrorCode, Result};
use crate::output::{Output, Rendered};
use clap::Args;
use serde_json::{json, Value};
use std::time::Duration;

pub const BUILDER_SCOPES: &[&str] = &[
    "records:read",
    "records:write",
    "records:delete",
    "tables:read",
    "tables:write",
    "schema:read",
    "schema:write",
    "apps:read",
    "workflows:read",
    "workflows:write",
    "workflows:execute",
    "reports:read",
    "reports:write",
    "files:read",
    "files:write",
    "pages:read",
    "pages:write",
    "branches:read",
];
const ELEVATED: &[&str] = &[
    "*",
    "admin:*",
    "admin:read",
    "admin:write",
    "public:read",
    "public:write",
    "public:submit",
    "public:publish",
];

#[derive(Args, Debug)]
pub struct LoginArgs {
    /// Platform base URL (default: the profile's, else https://apps.erp.ai)
    #[arg(long)]
    pub base_url: Option<String>,
    /// Store an existing API key instead of signing in through the browser
    #[arg(long, conflicts_with = "api_key_stdin")]
    pub api_key: Option<String>,
    /// Read the API key from stdin
    #[arg(long)]
    pub api_key_stdin: bool,
    /// Mint a key with only the *:read scopes
    #[arg(long, conflicts_with = "scopes")]
    pub read_only: bool,
    /// Mint a key with exactly these scopes (comma-separated)
    #[arg(long)]
    pub scopes: Option<String>,
    /// Mint an org-wide key (not restricted to specific apps); requires --yes when not on a terminal
    #[arg(long)]
    pub all_apps: bool,
    /// Restrict the minted key to these app ids (repeatable). The sign-in page can also choose them.
    #[arg(long)]
    pub app: Vec<String>,
    /// Print the sign-in URL instead of opening a browser
    #[arg(long)]
    pub no_browser: bool,
}

fn scopes_for(a: &LoginArgs) -> Result<Vec<String>> {
    let list: Vec<String> = if let Some(s) = &a.scopes {
        s.split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect()
    } else if a.read_only {
        BUILDER_SCOPES
            .iter()
            .filter(|s| s.ends_with(":read"))
            .map(|s| s.to_string())
            .collect()
    } else {
        BUILDER_SCOPES.iter().map(|s| s.to_string()).collect()
    };
    if let Some(bad) = list.iter().find(|s| ELEVATED.contains(&s.as_str())) {
        return Err(CliError::validation(format!("scope {bad} is elevated and cannot be minted by the CLI"))
            .with_hint("an org admin can create such a key at https://apps.erp.ai/apps?settings=api-keys; then `erpai login --api-key`"));
    }
    if list.is_empty() {
        return Err(CliError::validation("no scopes"));
    }
    Ok(list)
}

pub async fn login(g: &Global, a: LoginArgs) -> Result<Rendered> {
    let store = ProfileStore::open()?;
    let existing = store.load(&g.profile)?;
    let base = a
        .base_url
        .clone()
        .or_else(|| existing.as_ref().map(|p| p.base_url.clone()))
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let base = base.trim_end_matches('/').to_string();
    url::Url::parse(&base).map_err(|e| CliError::validation(format!("invalid --base-url: {e}")))?;
    // validate scopes early so a bad --scopes fails before any browser or network activity
    let scopes = scopes_for(&a)?;

    let mut profile = Profile::new(&g.profile, &base, "");
    if let Some(k) = key_from_args(&a)? {
        if k.is_empty() {
            return Err(CliError::validation("empty API key"));
        }
        profile.api_key = k;
        profile.key_name = Some("pasted".into());
    } else {
        if a.all_apps && !g.yes && !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            return Err(CliError::validation("--all-apps mints an org-wide key")
                .with_hint("pass --yes to confirm"));
        }
        let (listener, port) = callback::Listener::bind()?;
        let state = random_state();
        let url = format!("{base}/auth/cli?port={port}&state={state}");
        eprintln!("open: {url}");
        if !a.no_browser && open::that(&url).is_err() {
            eprintln!("could not open a browser — open the URL above manually");
        }
        let cb = listener.wait(&state, Duration::from_secs(300))?;
        let mut apps: Vec<String> = a.app.clone();
        apps.extend(cb.apps);
        apps.sort();
        apps.dedup();
        if apps.is_empty() && !a.all_apps {
            return Err(CliError::validation("no apps were selected for this key")
                .with_hint("pass --app <id> (repeatable) or --all-apps for an org-wide key"));
        }
        let anon = ApiClient::new(&Profile::new("tmp", &base, "none"))?;
        let ex = anon
            .post(
                "/v1/onboarding/desktop-auth/exchange",
                &[],
                &json!({ "code": cb.code }),
            )
            .await
            .map_err(|e| {
                if e.code == ErrorCode::NotFound {
                    CliError::auth("sign-in code expired or already used")
                        .with_hint("run `erpai login` again")
                } else {
                    e
                }
            })?;
        let token = ex
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::internal("exchange response without token"))?
            .to_string();
        profile.user_id = ex
            .pointer("/user/id")
            .and_then(Value::as_str)
            .map(str::to_string);
        profile.email = ex
            .pointer("/user/email")
            .and_then(Value::as_str)
            .map(str::to_string);
        profile.org_id = ex
            .pointer("/session/activeOrganizationId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let host = hostname().unwrap_or_else(|| "unknown-host".into());
        let label = if a.all_apps {
            "all apps".to_string()
        } else {
            apps.join(",")
        };
        let mut body = json!({
            "name": format!("erpai CLI · {host} · {label}"),
            "type": "pat", "environment": "live", "scopes": scopes,
        });
        if !a.all_apps {
            body["resourceRestrictions"] = json!({ "appIds": apps });
        }
        let session = ApiClient::new(&Profile::new("tmp", &base, &token))?;
        let created = session.post("/v1/onboarding/api-key", &[], &body).await?;
        profile.api_key = created
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::internal("key create response without key"))?
            .to_string();
        profile.key_id = created
            .get("id")
            .or(created.get("_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        profile.key_name = created
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string);
        profile.scopes = str_list(created.get("scopes"));
        profile.allowed_apps = str_list(created.pointer("/resourceRestrictions/appIds"));
        if profile.org_id.is_none() {
            profile.org_id = created
                .get("tenantId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }
    store.save(&profile)?;
    let mut id = profile.identity();
    id["source"] = "login".into();
    Ok(Output::item(id))
}

fn str_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn key_from_args(a: &LoginArgs) -> Result<Option<String>> {
    if let Some(k) = &a.api_key {
        return Ok(Some(k.trim().to_string()));
    }
    if a.api_key_stdin {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
        return Ok(Some(s.trim().to_string()));
    }
    Ok(None)
}

fn random_state() -> String {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    base16ct::lower::encode_string(&b)
}

fn hostname() -> Option<String> {
    if let Ok(h) = std::env::var("HOSTNAME") {
        if !h.trim().is_empty() {
            return Some(h.trim().to_string());
        }
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub async fn logout(g: &Global) -> Result<Rendered> {
    let store = ProfileStore::open()?;
    let Some(p) = store.load(&g.profile)? else {
        return Ok(Output::item(
            json!({ "revoked": false, "forgotten": false, "message": "no such profile" }),
        ));
    };
    let mut revoked = false;
    let mut detail = Value::Null;
    if let Some(id) = &p.key_id {
        let api = ApiClient::new(&p)?;
        match api
            .delete(&format!("/v1/onboarding/api-key/{id}"), &[], None)
            .await
        {
            Ok(_) => revoked = true,
            Err(e) => detail = json!({ "code": e.code.as_str(), "message": e.message }),
        }
    }
    store.delete(&g.profile)?;
    let mut out = json!({ "revoked": revoked, "forgotten": true, "profile": g.profile });
    if !revoked {
        out["hint"] = "the key is still active server-side — revoke it at https://apps.erp.ai/apps?settings=api-keys".into();
        out["detail"] = detail;
    }
    Ok(Output::item(out))
}

pub async fn whoami(g: &Global) -> Result<Rendered> {
    let store = ProfileStore::open()?;
    let mut p = require_profile(g)?;
    let api = ApiClient::new(&p)?;
    match api.get("/v1/app-builder/whoami", &[]).await {
        Ok(v) => {
            let body = v.get("data").cloned().unwrap_or(v);
            if let Some(s) = body.get("userId").and_then(Value::as_str) {
                p.user_id = Some(s.into());
            }
            if let Some(s) = body.get("email").and_then(Value::as_str) {
                p.email = Some(s.into());
            }
            if let Some(s) = body.get("tenantId").and_then(Value::as_str) {
                p.org_id = Some(s.into());
            }
            if let Some(s) = body.get("orgName").and_then(Value::as_str) {
                p.org_name = Some(s.into());
            }
            if let Some(k) = body.get("apiKey") {
                if let Some(s) = k.get("id").and_then(Value::as_str) {
                    p.key_id = Some(s.into());
                }
                if let Some(s) = k.get("name").and_then(Value::as_str) {
                    p.key_name = Some(s.into());
                }
                if k.get("scopes").is_some() {
                    p.scopes = str_list(k.get("scopes"));
                }
                if k.get("allowedApps").is_some() {
                    p.allowed_apps = str_list(k.get("allowedApps"));
                }
            }
            store.save(&p)?;
            let mut id = p.identity();
            id["source"] = "server".into();
            id["authMethod"] = body.get("authMethod").cloned().unwrap_or(Value::Null);
            Ok(Output::item(id))
        }
        Err(e) if e.code == ErrorCode::NotFound => {
            // older server without the endpoint: prove the credential with a read, report stored identity
            api.get("/v1/app-builder/app", &[("pageSize", "1")]).await?;
            let mut id = p.identity();
            id["source"] = "login".into();
            Ok(Output::item(id))
        }
        Err(e) => Err(e),
    }
}

pub async fn doctor(g: &Global) -> Result<Rendered> {
    let store = ProfileStore::open()?;
    let mut checks = vec![];
    let mut first_err: Option<CliError> = None;
    let profile = store.load(&g.profile)?;
    checks.push(json!({
        "name": "profile", "ok": profile.is_some(),
        "detail": profile.as_ref().map(|p| p.base_url.clone()).unwrap_or_else(|| "missing — run `erpai login`".into()),
    }));
    if let Some(p) = &profile {
        let base_ok = url::Url::parse(&p.base_url).is_ok();
        checks.push(json!({ "name": "baseUrl", "ok": base_ok, "detail": p.base_url }));
        match whoami(g).await {
            Ok(r) => checks.push(json!({ "name": "credential", "ok": true, "detail": r.to_json()["data"]["source"] })),
            Err(e) => {
                checks.push(json!({ "name": "credential", "ok": false, "detail": e.message }));
                first_err.get_or_insert(e);
            }
        }
    } else {
        first_err = Some(CliError::auth("no profile").with_hint("run `erpai login`"));
    }
    let all_ok = checks.iter().all(|c| c["ok"] == true);
    let out = Output::item(json!({
        "version": env!("CARGO_PKG_VERSION"), "profile": g.profile,
        "configDir": store.dir().display().to_string(), "ok": all_ok, "checks": checks,
    }));
    match (all_ok, first_err) {
        (true, _) => Ok(out),
        (false, Some(e)) => {
            out.print(g.format);
            Err(e)
        }
        (false, None) => Err(CliError::internal("doctor found a failing check")),
    }
}
