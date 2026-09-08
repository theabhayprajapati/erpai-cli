use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct TablesCmd {
    #[command(subcommand)]
    pub cmd: TablesSub,
}

#[derive(Subcommand, Debug)]
pub enum TablesSub {
    /// List tables of --app. Output: {data:[{_id,name,category,icon,…}], page}. Errors: validation_error (no --app), forbidden, auth_error.
    List {
        #[arg(long)]
        q: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 30)]
        page_size: u32,
    },
    /// Get one table with its columnsMetaData. Output: {data:{_id,name,columnsMetaData:[…]}}. Errors: not_found, forbidden.
    Get { table_id: String },
    /// Create a table with the same Id (auto_seq) and Name (text) columns the app creates, unless --no-default-columns. Output: {data:{id,name}} — the new table id is .data.id.
    Create {
        #[arg(long)]
        name: String,
        /// Create only the system columns (no Id / Name column)
        #[arg(long)]
        no_default_columns: bool,
        #[arg(long)]
        category: Option<String>,
        /// Lucide icon name in PascalCase, e.g. ShoppingCart
        #[arg(long)]
        icon: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Platform object type; omit for an ordinary table (the server picks its default)
        #[arg(long)]
        object_type: Option<String>,
    },
    /// Update name/description/category/icon from a JSON body. Output: {data:{…}}.
    Update {
        table_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Delete a table and all its records (destructive: needs --yes or a confirmation). Output: {data:{message}}.
    Delete { table_id: String },
    /// Custom action buttons of a table
    Actions {
        #[command(subcommand)]
        cmd: ActionsSub,
    },
}

#[derive(Subcommand, Debug)]
pub enum ActionsSub {
    /// List AUTO_BUILDER custom actions. Output: {data:[{id,title,actionName,…}]} — `id` is what a workflow trigger's customActionName must hold.
    List { table_id: String },
    /// Create a custom action button. Output: {data:{id,title,…}} — the id is server-generated.
    Create {
        table_id: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        success_message: Option<String>,
        #[arg(long)]
        error_message: Option<String>,
    },
    /// Delete a custom action (destructive). Output: {data:{message}}.
    Delete { table_id: String, action_id: String },
}

pub async fn run(g: &Global, c: TablesCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        TablesSub::List {
            q: search,
            page,
            page_size,
        } => {
            let mut pairs = page_args(page, page_size)?;
            pairs.push(("appId".into(), app.clone()));
            if let Some(s) = search {
                pairs.push(("q".into(), s));
            }
            let v = api.get("/v1/app-builder/table", &q(&pairs)).await?;
            Ok(list_from(&v, page, page_size).with_context(cx))
        }
        TablesSub::Get { table_id } => Ok(item_from(
            api.get(&format!("/v1/app-builder/table/{table_id}"), &app_q)
                .await?,
        )
        .with_context(cx)),
        TablesSub::Create {
            name,
            category,
            icon,
            description,
            object_type,
            no_default_columns,
        } => {
            // the create endpoint expects appId in the body as well as the query string
            let mut body = serde_json::json!({ "name": name, "appId": app });
            if !no_default_columns {
                body["columnsMetaData"] = serde_json::json!([
                    { "id": "ID", "type": "auto_seq", "required": false, "name": "Id", "width": 150, "columnCode": "id", "editable": true },
                    { "id": "NAME", "type": "text", "required": false, "name": "Name", "width": 200, "columnCode": "name", "editable": true }
                ]);
            }
            if let Some(o) = object_type {
                body["objectType"] = o.into();
            }
            if let Some(c) = category {
                body["category"] = c.into();
            }
            if let Some(i) = icon {
                body["icon"] = i.into();
            }
            if let Some(d) = description {
                body["description"] = d.into();
            }
            if gate.dry_run {
                return Ok(dry_run_plan(
                    "POST",
                    "/v1/app-builder/table",
                    Some(&body),
                    cx,
                ));
            }
            Ok(item_from(api.post("/v1/app-builder/table", &app_q, &body).await?).with_context(cx))
        }
        TablesSub::Update {
            table_id,
            body,
            file,
        } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            let path = format!("/v1/app-builder/table/{table_id}");
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        TablesSub::Delete { table_id } => {
            let path = format!("/v1/app-builder/table/{table_id}");
            match gate.confirm(&format!("delete table {table_id} and all its records"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("table {table_id} deleted")).with_context(cx))
                }
            }
        }
        TablesSub::Actions { cmd } => match cmd {
            ActionsSub::List { table_id } => {
                let v = api
                    .get(&format!("/v1/app-builder/table/{table_id}"), &app_q)
                    .await?;
                let actions: Vec<Value> = inner(&v)
                    .get("customActions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|a| a.get("type").and_then(Value::as_str) == Some("AUTO_BUILDER"))
                    .collect();
                Ok(Output::list(actions, None).with_context(cx))
            }
            ActionsSub::Create {
                table_id,
                title,
                success_message,
                error_message,
            } => {
                let body = serde_json::json!({
                    "title": title, "type": "AUTO_BUILDER", "actionName": title,
                    "successMessage": success_message.unwrap_or_else(|| "Action completed successfully".into()),
                    "errorMessage": error_message.unwrap_or_else(|| "Action failed".into()),
                });
                let path = format!("/v1/app-builder/table/{table_id}/custom-action");
                if gate.dry_run {
                    return Ok(dry_run_plan("POST", &path, Some(&body), cx));
                }
                Ok(item_from(api.post(&path, &app_q, &body).await?).with_context(cx))
            }
            ActionsSub::Delete {
                table_id,
                action_id,
            } => {
                let path = format!("/v1/app-builder/table/{table_id}/custom-action/{action_id}");
                match gate.confirm(
                    &format!("delete custom action {action_id} on table {table_id}"),
                    &cx,
                )? {
                    Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                    Decision::Proceed => {
                        api.delete(&path, &app_q, None).await?;
                        Ok(Output::message(format!("action {action_id} deleted")).with_context(cx))
                    }
                }
            }
        },
    }
}
