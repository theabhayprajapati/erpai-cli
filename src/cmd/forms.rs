use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct FormsCmd {
    #[command(subcommand)]
    pub cmd: FormsSub,
}

#[derive(Subcommand, Debug)]
pub enum FormsSub {
    /// The entry form of a table. Output: {data:{title,fields:[…],formSetting?}}.
    Get {
        #[arg(long)]
        table: String,
    },
    /// Every entry form in the app. Output: {data:[…]}.
    List,
    /// Create or replace a table's entry form from {title,fields:[{_id,title,type,required,index,uiVisible,readOnly,…}]}. Output: {data:{…}}.
    Set {
        #[arg(long)]
        table: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Replace the form (PUT). Output: {data:{…}}.
    Update {
        #[arg(long)]
        table: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Partial update, e.g. --body '{"formSetting":{"showHeader":true}}'. Output: {data:{…}}.
    Patch {
        #[arg(long)]
        table: String,
        #[arg(long)]
        body: String,
    },
    /// Delete the form (destructive; the UI falls back to all columns). Output: {data:{message}}.
    Delete {
        #[arg(long)]
        table: String,
    },
}

fn require_fields(body: &Value) -> Result<()> {
    if body.get("fields").map(Value::is_array).unwrap_or(false) {
        Ok(())
    } else {
        Err(CliError::validation("form body needs a fields array").with_hint("each field: {_id,title,type:column_view|section|table,required,index,uiVisible,readOnly}"))
    }
}

pub async fn run(g: &Global, c: FormsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    let form = |t: &str| format!("/v1/app-builder/table/{t}/entry-form");
    match c.cmd {
        FormsSub::Get { table } => {
            Ok(item_from(api.get(&form(&table), &app_q).await?).with_context(cx))
        }
        FormsSub::List => {
            let v = api
                .get(&format!("/v1/app-builder/app/{app}/entry-form"), &[])
                .await?;
            let data = v
                .get("body")
                .or(v.get("data"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(Output::list(data, None).with_context(cx))
        }
        FormsSub::Set { table, body, file } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            require_fields(&body)?;
            let path = form(&table);
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &app_q, &body).await?).with_context(cx))
        }
        FormsSub::Update { table, body, file } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            require_fields(&body)?;
            let path = form(&table);
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        FormsSub::Patch { table, body } => {
            let body: Value = serde_json::from_str(&body)?;
            let path = form(&table);
            if gate.dry_run {
                return Ok(dry_run_plan("PATCH", &path, Some(&body), cx));
            }
            Ok(item_from(api.patch(&path, &app_q, &body).await?).with_context(cx))
        }
        FormsSub::Delete { table } => {
            let path = form(&table);
            match gate.confirm(&format!("delete the entry form of table {table}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("entry form of {table} deleted")).with_context(cx))
                }
            }
        }
    }
}
