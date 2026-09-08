use super::*;
use crate::safety::{preflight_app, resolve_app};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct SqlCmd {
    #[command(subcommand)]
    pub cmd: SqlSub,
}

#[derive(Subcommand, Debug)]
pub enum SqlSub {
    /// List the app's SQL views and their columns. Output: {data:[{tableName,columns:[{columnName,dataType}]}]}.
    Schema,
    /// Run a SELECT (ClickHouse dialect) over the app's views. Output: {data:{rows,fields,rowCount}}. Errors: validation_error (not a SELECT), api_error.
    Run {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    /// Generate SQL from a natural-language prompt (review before running). Output: {data:{query,columns}}.
    Generate {
        #[arg(long)]
        prompt: String,
    },
}

pub async fn run(g: &Global, c: SqlCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    match c.cmd {
        SqlSub::Schema => {
            let v = api
                .get("/v1/agent/app/sql/tables", &[("appId", app.as_str())])
                .await?;
            let data = list_items(&v);
            Ok(Output::list(data, None).with_context(cx))
        }
        SqlSub::Run { query, file, limit } => {
            let sql = match (query, file) {
                (Some(q), None) => q,
                (None, Some(f)) => std::fs::read_to_string(f)?,
                _ => {
                    return Err(CliError::validation(
                        "pass --query '<sql>' or --file <path>",
                    ))
                }
            };
            let first = sql
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            if first != "SELECT" && first != "WITH" {
                return Err(
                    CliError::validation("only SELECT queries are allowed").with_hint(
                        "the SQL surface is read-only; use `erpai records …` to change data",
                    ),
                );
            }
            let body = serde_json::json!({ "appId": app, "sqlQuery": sql, "limit": limit });
            Ok(
                item_from(api.post("/v1/agent/app/sql/execute", &[], &body).await?)
                    .with_context(cx),
            )
        }
        SqlSub::Generate { prompt } => {
            let body = serde_json::json!({ "appId": app, "prompt": prompt });
            Ok(
                item_from(api.post("/v1/agent/app/sql/generate", &[], &body).await?)
                    .with_context(cx),
            )
        }
    }
}
