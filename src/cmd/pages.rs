use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

const BASE: &str = "/v1/agent/app/custom-pages";

#[derive(Args, Debug)]
pub struct PagesCmd {
    #[command(subcommand)]
    pub cmd: PagesSub,
}

#[derive(Subcommand, Debug)]
pub enum PagesSub {
    /// List custom pages. Output: {data:[{_id,name,slug,category,pageType?}]}.
    List {
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Get a page by slug. Output: {data:{_id,name,slug,html,…}}.
    Get { slug: String },
    /// Get a page by slug with its pre-fetched data. Output: {data:{…}}.
    Hydrated { slug: String },
    /// Resolve the home page config for the current user (or --role). Output: {data:{…}}.
    Home {
        #[arg(long)]
        role: Option<String>,
    },
    /// Create a custom HTML page. The HTML must use window.ERPAI, never a key literal. Output: {data:{_id,slug,…}}.
    Create {
        #[arg(long)]
        name: String,
        /// lowercase, hyphen-separated
        #[arg(long)]
        slug: String,
        #[arg(long)]
        html_file: PathBuf,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        icon: Option<String>,
        #[arg(long)]
        category: Option<String>,
    },
    /// Create a home page from a config file ({greeting,sections:[…]}). Output: {data:{_id,…}}.
    CreateHome {
        #[arg(long)]
        config_file: PathBuf,
        #[arg(long, default_value = "Home")]
        name: String,
    },
    /// Update a page from a JSON body and/or --html-file. Output: {data:{…}}.
    Update {
        page_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        html_file: Option<PathBuf>,
    },
    /// Delete a page (destructive). Output: {data:{message}}.
    Delete { page_id: String },
    /// Reorder pages by id. Output: {data:{…}}.
    Reorder {
        #[arg(required = true)]
        page_ids: Vec<String>,
    },
}

pub fn check_html(html: &str) -> Result<()> {
    if html.contains("erp_pat_live_")
        || html.contains("erp_org_live_")
        || html.contains("erp_pat_test_")
    {
        return Err(CliError::validation("the HTML embeds an API key literal")
            .with_hint("saved pages must use window.ERPAI / the erpai page SDK; the platform injects auth at render time"));
    }
    Ok(())
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

pub async fn run(g: &Global, c: PagesCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        PagesSub::List { limit } => {
            let lim = limit.to_string();
            let v = api
                .get(BASE, &[("appId", app.as_str()), ("limit", lim.as_str())])
                .await?;
            let data = v
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(Output::list(data, None).with_context(cx))
        }
        PagesSub::Get { slug } => {
            Ok(item_from(api.get(&format!("{BASE}/slug/{slug}"), &app_q).await?).with_context(cx))
        }
        PagesSub::Hydrated { slug } => Ok(item_from(
            api.get(&format!("{BASE}/slug/{slug}/hydrated"), &app_q)
                .await?,
        )
        .with_context(cx)),
        PagesSub::Home { role } => {
            let mut pairs = vec![("appId".to_string(), app.clone())];
            if let Some(r) = role {
                pairs.push(("roleId".into(), r));
            }
            Ok(
                item_from(api.get(&format!("{BASE}/home/resolve"), &q(&pairs)).await?)
                    .with_context(cx),
            )
        }
        PagesSub::Create {
            name,
            slug,
            html_file,
            description,
            icon,
            category,
        } => {
            if !valid_slug(&slug) {
                return Err(CliError::validation(format!(
                    "slug '{slug}' must be lowercase letters/digits separated by single hyphens"
                )));
            }
            let html = std::fs::read_to_string(&html_file)?;
            check_html(&html)?;
            let mut body = serde_json::json!({ "name": name, "slug": slug, "html": html });
            if let Some(d) = description {
                body["description"] = d.into();
            }
            if let Some(i) = icon {
                body["icon"] = i.into();
            }
            if let Some(c) = category {
                body["category"] = c.into();
            }
            if gate.dry_run {
                return Ok(dry_run_plan("POST", BASE, Some(&body), cx));
            }
            Ok(item_from(api.post(BASE, &app_q, &body).await?).with_context(cx))
        }
        PagesSub::CreateHome { config_file, name } => {
            let config = read_json_arg(None, Some(&config_file))?;
            if !config.is_object() {
                return Err(CliError::validation(
                    "--config-file must contain a JSON object",
                ));
            }
            let body = serde_json::json!({ "name": name, "pageType": "home", "config": config });
            if gate.dry_run {
                return Ok(dry_run_plan("POST", BASE, Some(&body), cx));
            }
            Ok(item_from(api.post(BASE, &app_q, &body).await?).with_context(cx))
        }
        PagesSub::Update {
            page_id,
            body,
            file,
            html_file,
        } => {
            let mut body = match (&body, &file) {
                (None, None) => serde_json::json!({}),
                _ => read_json_arg(body.as_deref(), file.as_deref())?,
            };
            if let Some(h) = html_file {
                let html = std::fs::read_to_string(h)?;
                check_html(&html)?;
                body["html"] = html.into();
            }
            if body.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                return Err(CliError::validation("nothing to update")
                    .with_hint("pass --body/--file and/or --html-file"));
            }
            let path = format!("{BASE}/{page_id}");
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        PagesSub::Delete { page_id } => {
            let path = format!("{BASE}/{page_id}");
            match gate.confirm(&format!("delete page {page_id}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("page {page_id} deleted")).with_context(cx))
                }
            }
        }
        PagesSub::Reorder { page_ids } => {
            let path = format!("{BASE}/reorder");
            let body = serde_json::json!({ "pageIds": page_ids });
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &app_q, &body).await?).with_context(cx))
        }
    }
}
