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
    /// Published catalog entries. Output: {data:[{slug,title,…}]}.
    List,
    /// A published catalog entry by slug (--publisher <namespace> for a tenant publisher's product). Output: {data:{slug,title,status,publicationFeatures,…}}.
    Get {
        slug: String,
        #[arg(long)]
        publisher: Option<String>,
    },
    /// Publisher-program state and settings (overview, terms, profile, namespace claim).
    Publisher(PublisherCmd),
    /// The publication that --app is the source of, if any. Output: {data:{…}}.
    SourcePublication,
    /// The unpublished draft for --app, if any. Output: {data:{…}}.
    SourceDraft,
    /// Save a catalog draft for --app (appId injected; partial content allowed). Output: {data:{…}}.
    Draft {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Validate and render the listing without publishing (appId injected). Output: {data:{…}} or the exact validation errors.
    Preview {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Publish (or republish) the listing for --app (appId injected; delivery is asynchronous). Output: {data:{…}}.
    Publish {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Unpublish --app's listing (destructive). Output: {data:{…}}.
    Unpublish { slug: String },
    /// Activate a published product into the current org (creates or reuses the runtime app; confirmed). Output: {data:{runtimeAppId,activation:{id,status,publicationRevision},reused,…}}.
    Activate {
        slug: String,
        #[arg(long)]
        publisher: Option<String>,
        /// Optional runtime app name
        #[arg(long)]
        name: Option<String>,
    },
    /// Copy a published template into the current org as a new app (confirmed). Output: {data:{newAppId,copyJobId,…}}.
    Install {
        slug: String,
        #[arg(long)]
        publisher: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct PublisherCmd {
    #[command(subcommand)]
    pub cmd: PublisherSub,
}

#[derive(Subcommand, Debug)]
pub enum PublisherSub {
    /// Entitlement, publisher identity, terms state, profile, apps and publications. Output: {data:{entitlement,publisher,terms,profile,apps,publications}}.
    Overview,
    /// Current publisher terms. Output: {data:{current:{version,contentDigest,body},acceptanceRequired,…}}.
    Terms,
    /// Accept the current terms (version + contentDigest from `terms`, both required). Output: {data:{…}}.
    AcceptTerms {
        #[arg(long)]
        version: String,
        #[arg(long)]
        content_digest: String,
    },
    /// Public publisher profile. Output: {data:{profile:{bio,websiteUrl,supportEmail,logoUrl,expertise}}}.
    Profile,
    /// Set the public profile from {profile:{…}}. Output: {data:{…}}.
    SetProfile {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Claim the permanent publisher namespace (cannot be renamed later). Output: {data:{…}}.
    Claim {
        #[arg(long)]
        display_name: String,
        #[arg(long)]
        namespace: String,
    },
}

fn product_path(slug: &str, publisher: Option<&str>, action: &str) -> String {
    match publisher {
        Some(ns) => format!("{BASE}/publishers/{ns}/{slug}{action}"),
        None => format!("{BASE}/{slug}{action}"),
    }
}

pub async fn run(g: &Global, c: CatalogCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let gate = Gate::from(g);
    // An explicitly named app is checked against the key's allowed apps even where the
    // command itself is org-level, so a wrong --app never sends a request.
    if let Some(a) = g.app.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        preflight_app(&p, a)?;
    }
    // Reading the catalog and activating a product are org-level: no --app.
    match c.cmd {
        CatalogSub::List => {
            let cx = context(&p, None);
            let v = api.get(BASE, &[]).await?;
            return Ok(list_from(&v, 1, 0).with_context(cx));
        }
        CatalogSub::Get { slug, publisher } => {
            let cx = context(&p, None);
            let path = product_path(&slug, publisher.as_deref(), "");
            return Ok(item_from(api.get(&path, &[]).await?).with_context(cx));
        }
        CatalogSub::Publisher(pc) => return run_publisher(&p, &api, &gate, pc).await,
        CatalogSub::Activate {
            slug,
            publisher,
            name,
        } => {
            let cx = context(&p, None);
            let path = product_path(&slug, publisher.as_deref(), "/activate");
            let body = match name {
                Some(n) => serde_json::json!({ "name": n }),
                None => serde_json::json!({}),
            };
            return match gate.confirm(
                &format!("activate product {slug} into org {}", p.org_label()),
                &cx,
            )? {
                Decision::DryRun => Ok(dry_run_plan("POST", &path, Some(&body), cx)),
                Decision::Proceed => {
                    Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
                }
            };
        }
        CatalogSub::Install {
            slug,
            publisher,
            body,
        } => {
            let cx = context(&p, None);
            let path = product_path(&slug, publisher.as_deref(), "/install");
            let body: Value = match body {
                Some(b) => serde_json::from_str(&b)?,
                None => serde_json::json!({}),
            };
            return match gate.confirm(
                &format!("install template {slug} into org {}", p.org_label()),
                &cx,
            )? {
                Decision::DryRun => Ok(dry_run_plan("POST", &path, Some(&body), cx)),
                Decision::Proceed => {
                    Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
                }
            };
        }
        _ => {}
    }
    // Everything else is about --app as the publication source.
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let with_app = |mut b: Value| {
        if b.get("appId").is_none() {
            b["appId"] = app.clone().into();
        }
        b
    };
    match c.cmd {
        CatalogSub::SourcePublication => Ok(item_from(
            api.get(&format!("{BASE}/source-app/{app}/publication"), &[])
                .await?,
        )
        .with_context(cx)),
        CatalogSub::SourceDraft => Ok(item_from(
            api.get(&format!("{BASE}/source-app/{app}/draft"), &[])
                .await?,
        )
        .with_context(cx)),
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
            let path = format!("{BASE}/publish/preview-v1");
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
        }
        CatalogSub::Publish { body, file } => {
            let body = with_app(read_json_arg(body.as_deref(), file.as_deref())?);
            let path = format!("{BASE}/publish");
            match gate.confirm(&format!("publish app {app} to the public catalog"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("POST", &path, Some(&body), cx)),
                Decision::Proceed => {
                    Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
                }
            }
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
        CatalogSub::List
        | CatalogSub::Get { .. }
        | CatalogSub::Publisher(_)
        | CatalogSub::Activate { .. }
        | CatalogSub::Install { .. } => unreachable!("handled above"),
    }
}

async fn run_publisher(
    p: &Profile,
    api: &ApiClient,
    gate: &Gate,
    c: PublisherCmd,
) -> Result<Rendered> {
    let cx = context(p, None);
    match c.cmd {
        PublisherSub::Overview => Ok(item_from(
            api.get(&format!("{BASE}/publisher/overview"), &[]).await?,
        )
        .with_context(cx)),
        PublisherSub::Terms => {
            Ok(item_from(api.get(&format!("{BASE}/publisher/terms"), &[]).await?).with_context(cx))
        }
        PublisherSub::AcceptTerms {
            version,
            content_digest,
        } => {
            let path = format!("{BASE}/publisher/terms/accept");
            let body = serde_json::json!({ "version": version, "contentDigest": content_digest });
            match gate.confirm(&format!("accept publisher terms {version}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("POST", &path, Some(&body), cx)),
                Decision::Proceed => {
                    Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
                }
            }
        }
        PublisherSub::Profile => Ok(item_from(
            api.get(&format!("{BASE}/publisher/profile"), &[]).await?,
        )
        .with_context(cx)),
        PublisherSub::SetProfile { body, file } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            let path = format!("{BASE}/publisher/profile");
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &[], &body).await?).with_context(cx))
        }
        PublisherSub::Claim {
            display_name,
            namespace,
        } => {
            let path = format!("{BASE}/publisher");
            let body = serde_json::json!({ "displayName": display_name, "namespace": namespace });
            match gate.confirm(
                &format!("claim the permanent publisher namespace '{namespace}'"),
                &cx,
            )? {
                Decision::DryRun => Ok(dry_run_plan("PUT", &path, Some(&body), cx)),
                Decision::Proceed => {
                    Ok(item_from(api.put(&path, &[], &body).await?).with_context(cx))
                }
            }
        }
    }
}
