use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ColumnsCmd {
    #[command(subcommand)]
    pub cmd: ColumnsSub,
}

const ADD_HELP: &str = "Body: {name, type, options?[{name,color}], refTable?{_id,colId}, formula?{expression,variablePath}, required?, hidden?}. Types: text, long_text, number, date, select, multi-select, ref, formula, rollup, checkbox, url, email, phone, attachment, user. Pass a JSON array to add several columns in one call.";

#[derive(Subcommand, Debug)]
pub enum ColumnsSub {
    /// List a table's columns. Output: {data:[{id,name,type,columnCode,required,hidden,options?,refTable?}]}.
    List { table_id: String },
    /// Add one column (JSON object) or many (JSON array). Output: {data:{id,…}} or {data:[…]}.
    #[command(after_help = ADD_HELP)]
    Add {
        table_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Update a column (name, required, hidden, options, …; type cannot change). Output: {data:{…}}.
    Update {
        table_id: String,
        column_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Delete a column and its data (destructive). Output: {data:{message}}.
    Delete { table_id: String, column_id: String },
}

pub async fn run(g: &Global, c: ColumnsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        ColumnsSub::List { table_id } => {
            let v = api
                .get(&format!("/v1/app-builder/table/{table_id}"), &app_q)
                .await?;
            let cols: Vec<Value> = inner(&v)
                .get("columnsMetaData")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|c| {
                    let mut o = serde_json::json!({
                        "id": c["id"], "name": c["name"], "type": c["type"], "columnCode": c["columnCode"],
                        "required": c.get("required").cloned().unwrap_or(Value::Bool(false)),
                        "hidden": c.get("hidden").cloned().unwrap_or(Value::Bool(false)),
                    });
                    for k in ["options", "refTable", "formula", "systemField"] {
                        if let Some(x) = c.get(k) {
                            o[k] = x.clone();
                        }
                    }
                    o
                })
                .collect();
            Ok(Output::list(cols, None).with_context(cx))
        }
        ColumnsSub::Add {
            table_id,
            body,
            file,
        } => {
            let mut body = read_json_arg(body.as_deref(), file.as_deref())?;
            fill_option_ids(&mut body);
            let (path, payload) = if body.is_array() {
                (
                    format!("/v1/app-builder/table/{table_id}/column/bulk"),
                    serde_json::json!({ "columns": body }),
                )
            } else if body.is_object() {
                (format!("/v1/app-builder/table/{table_id}/column"), body)
            } else {
                return Err(CliError::validation(
                    "column body must be a JSON object or an array of objects",
                ));
            };
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&payload), cx));
            }
            Ok(item_from(api.post(&path, &app_q, &payload).await?).with_context(cx))
        }
        ColumnsSub::Update {
            table_id,
            column_id,
            body,
            file,
        } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            let path = format!("/v1/app-builder/table/{table_id}/column/{column_id}");
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        ColumnsSub::Delete {
            table_id,
            column_id,
        } => {
            let path = format!("/v1/app-builder/table/{table_id}/column/{column_id}");
            match gate.confirm(
                &format!("delete column {column_id} (and its data) from table {table_id}"),
                &cx,
            )? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("column {column_id} deleted")).with_context(cx))
                }
            }
        }
    }
}

/// Select options are stored by id; the API requires one per option. Assign the
/// 1-based position when the caller left it out (the ids select cells refer to).
fn fill_option_ids(body: &mut Value) {
    let cols: Vec<&mut Value> = match body {
        Value::Array(a) => a.iter_mut().collect(),
        other => vec![other],
    };
    for col in cols {
        let Some(opts) = col.get_mut("options").and_then(Value::as_array_mut) else {
            continue;
        };
        for (i, o) in opts.iter_mut().enumerate() {
            if o.is_object() && o.get("id").is_none() {
                o["id"] = Value::String((i + 1).to_string());
            }
        }
    }
}
