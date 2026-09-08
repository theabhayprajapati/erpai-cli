use super::*;
use crate::cmd::pages::check_html;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

const BASE: &str = "/v1/agent/app/table-html-widgets";

#[derive(Args, Debug)]
pub struct WidgetsCmd {
    #[command(subcommand)]
    pub cmd: WidgetsSub,
}

#[derive(Subcommand, Debug)]
pub enum WidgetsSub {
    /// The insights widget above a table. Output: {data:{_id,tableId,html}}.
    Get {
        #[arg(long)]
        table: String,
    },
    /// All widgets in the app. Output: {data:[…]}.
    List,
    /// Create or replace a table's widget from an HTML file (upsert). Output: {data:{_id,…}}.
    Set {
        #[arg(long)]
        table: String,
        #[arg(long)]
        html_file: PathBuf,
    },
    /// Update a widget's HTML. Output: {data:{…}}.
    Update {
        widget_id: String,
        #[arg(long)]
        html_file: PathBuf,
    },
    /// Delete a widget (destructive). Output: {data:{message}}.
    Delete { widget_id: String },
}

pub async fn run(g: &Global, c: WidgetsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        WidgetsSub::Get { table } => Ok(item_from(
            api.get(&format!("{BASE}/table/{table}"), &app_q).await?,
        )
        .with_context(cx)),
        WidgetsSub::List => {
            let v = api.get(BASE, &app_q).await?;
            let data = v
                .get("data")
                .or(v.get("body"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(Output::list(data, None).with_context(cx))
        }
        WidgetsSub::Set { table, html_file } => {
            let html = std::fs::read_to_string(&html_file)?;
            check_html(&html)?;
            let body = serde_json::json!({ "tableId": table, "html": html });
            if gate.dry_run {
                return Ok(dry_run_plan("POST", BASE, Some(&body), cx));
            }
            Ok(item_from(api.post(BASE, &app_q, &body).await?).with_context(cx))
        }
        WidgetsSub::Update {
            widget_id,
            html_file,
        } => {
            let html = std::fs::read_to_string(&html_file)?;
            check_html(&html)?;
            let path = format!("{BASE}/{widget_id}");
            let body = serde_json::json!({ "html": html });
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        WidgetsSub::Delete { widget_id } => {
            let path = format!("{BASE}/{widget_id}");
            match gate.confirm(&format!("delete widget {widget_id}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("widget {widget_id} deleted")).with_context(cx))
                }
            }
        }
    }
}
