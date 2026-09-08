use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

const BASE: &str = "/v1/app-builder/template-catalog";

#[derive(Args, Debug)]
pub struct CatalogCmd {
    #[command(subcommand)]
    pub cmd: CatalogSub,
}

#[derive(Subcommand, Debug)]
pub enum CatalogSub {
    /// A published catalog entry by slug. Output: {data:{…}}.
    Get { slug: String },
    /// The catalog entry that --app is the source of, if any. Output: {data:{…}}.
    SourceApp,
    /// Save a catalog draft for --app (appId injected). Output: {data:{…}}.
    Draft {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Render a preview of the listing. Output: {data:{…}}.
    Preview {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Publish (or republish) the listing. Output: {data:{…}}.
    Publish {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Unpublish a listing (destructive). Output: {data:{…}}.
    Unpublish { slug: String },
    /// Install a published template into the current org. Output: {data:{…}}.
    Install {
        slug: String,
        #[arg(long)]
        body: Option<String>,
    },
}

pub async fn run(g: &Global, c: CatalogCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    let with_app = |mut b: Value| {
        if b.get("appId").is_none() {
            b["appId"] = app.clone().into();
        }
        b
    };
    match c.cmd {
        CatalogSub::Get { slug } => {
            Ok(item_from(api.get(&format!("{BASE}/{slug}"), &[]).await?).with_context(cx))
        }
        CatalogSub::SourceApp => {
            Ok(item_from(api.get(&format!("{BASE}/source-app"), &app_q).await?).with_context(cx))
        }
        CatalogSub::Draft { body, file } => {
            let body = with_app(read_json_arg(body.as_deref(), file.as_deref())?);
            let path = format!("{BASE}/publish/draft");
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
        }
        CatalogSub::Preview { body, file } => {
            let body = with_app(read_json_arg(body.as_deref(), file.as_deref())?);
            let path = format!("{BASE}/publish/preview");
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
        }
        CatalogSub::Publish { body, file } => {
            let body = with_app(read_json_arg(body.as_deref(), file.as_deref())?);
            let path = format!("{BASE}/publish");
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
        }
        CatalogSub::Unpublish { slug } => {
            let path = format!("{BASE}/{slug}/unpublish");
            match gate.confirm(&format!("unpublish catalog listing {slug}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("POST", &path, None, cx)),
                Decision::Proceed => Ok(item_from(
                    api.post(&path, &[], &serde_json::json!({})).await?,
                )
                .with_context(cx)),
            }
        }
        CatalogSub::Install { slug, body } => {
            let body: Value = match body {
                Some(b) => serde_json::from_str(&b)?,
                None => serde_json::json!({}),
            };
            let path = format!("{BASE}/{slug}/install");
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
        }
    }
}
