use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

const LAYOUT_TYPES: &[&str] = &["tabular", "kanban", "calendar", "gallery", "timeline"];

#[derive(Args, Debug)]
pub struct LayoutsCmd {
    #[command(subcommand)]
    pub cmd: LayoutsSub,
}

#[derive(Subcommand, Debug)]
pub enum LayoutsSub {
    /// List a table's saved views. Output: {data:[{_id,name,viewType,…}]}.
    List {
        #[arg(long)]
        table: String,
    },
    /// Get one layout. Output: {data:{_id,name,viewType,config,…}}.
    Get { layout_id: String },
    /// Create a view from {tableId,name,viewType,config,default?}. viewType: tabular|kanban|calendar|gallery|timeline (layoutType accepted too). Output: {data:{_id,…}}.
    Create {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Update name/config/default. Output: {data:{…}}.
    Update {
        layout_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Delete a view (destructive; records are untouched). Output: {data:{message}}.
    Delete { layout_id: String },
}

pub async fn run(g: &Global, c: LayoutsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        LayoutsSub::List { table } => {
            let v = api
                .get(
                    "/v1/app-builder/layout",
                    &[("tableId", table.as_str()), ("appId", app.as_str())],
                )
                .await?;
            let data = list_items(&v);
            Ok(Output::list(data, None).with_context(cx))
        }
        LayoutsSub::Get { layout_id } => Ok(item_from(
            api.get(&format!("/v1/app-builder/layout/{layout_id}"), &app_q)
                .await?,
        )
        .with_context(cx)),
        LayoutsSub::Create { body, file } => {
            let mut body = read_json_arg(body.as_deref(), file.as_deref())?;
            for k in ["tableId", "name", "config"] {
                if body.get(k).is_none() {
                    return Err(CliError::validation(format!("layout body needs {k}")));
                }
            }
            // The platform's create endpoint reads `viewType`; docs historically said `layoutType`.
            // Accept either, validate the value, and send both so either server version is happy.
            let lt = body
                .get("viewType")
                .or(body.get("layoutType"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !LAYOUT_TYPES.contains(&lt.as_str()) {
                return Err(CliError::validation(format!(
                    "viewType '{lt}' is not one of {}",
                    LAYOUT_TYPES.join(", ")
                )));
            }
            body["viewType"] = Value::String(lt.clone());
            body["layoutType"] = Value::String(lt);
            if gate.dry_run {
                return Ok(dry_run_plan(
                    "POST",
                    "/v1/app-builder/layout",
                    Some(&body),
                    cx,
                ));
            }
            Ok(
                item_from(api.post("/v1/app-builder/layout", &app_q, &body).await?)
                    .with_context(cx),
            )
        }
        LayoutsSub::Update {
            layout_id,
            body,
            file,
        } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            let path = format!("/v1/app-builder/layout/{layout_id}");
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        LayoutsSub::Delete { layout_id } => {
            let path = format!("/v1/app-builder/layout/{layout_id}");
            match gate.confirm(&format!("delete view {layout_id}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("view {layout_id} deleted")).with_context(cx))
                }
            }
        }
    }
}
