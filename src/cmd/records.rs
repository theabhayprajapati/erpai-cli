use super::*;
use crate::safety::{preflight_app, require_filter, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct RecordsCmd {
    #[command(subcommand)]
    pub cmd: RecordsSub,
}

const FILTER_HELP: &str = "Filter JSON: {\"conditions\":[{\"colId\":\"<column-id>\",\"opr\":\"eq|neq|c|nc|sw|ew|gt|lt|gte|lte|emp|nemp|in|nin\",\"value\":…}],\"logicalOperator\":\"and|or\"}. Select values are 1-based arrays ([1]); dates are ISO 8601; CTDT/UTDT/CTBY/UTBY are the system columns.";

#[derive(Subcommand, Debug)]
pub enum RecordsSub {
    /// Read matching rows, server-side filtered and paged. Output: {data:[{_id,cells,createdAt,updatedAt}], page:{no,size,total}}.
    #[command(after_help = FILTER_HELP)]
    Query {
        table_id: String,
        #[arg(long)]
        filter: Option<String>,
        /// Full-text search across text columns
        #[arg(long)]
        q: Option<String>,
        #[arg(long, default_value = "CTDT")]
        sort_col: String,
        #[arg(long, default_value_t = -1, allow_hyphen_values = true)]
        sort_dir: i8,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 200)]
        page_size: u32,
        /// Expand reference columns into linked record data
        #[arg(long)]
        expand_refs: bool,
    },
    /// Count matching rows. Output: {data:{count}}.
    #[command(after_help = FILTER_HELP)]
    Count {
        table_id: String,
        #[arg(long)]
        filter: Option<String>,
    },
    /// Sum/group/stats on the server. Body: {aggregations:[{op:count|sum|avg|min|max,columnId?,alias}],groupBy?:[…],filter?}. Output: {data:{rows:[{group,values}]}}.
    Aggregate {
        table_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// One record. Output: {data:{_id,cells,…}}.
    Get { table_id: String, record_id: String },
    /// Many records by id. Output: {data:[…]}.
    GetMany {
        table_id: String,
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Create one record from cells {columnId: value}. Output: {data:{_id,…}}.
    Create {
        table_id: String,
        #[arg(long)]
        cells: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Create many records from a JSON array (each {cells:{…}} or a bare cells object; max 500). Output: {data:…}.
    BulkCreate {
        table_id: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Update cells of one record (other cells unchanged). Output: {data:{…}}.
    Update {
        table_id: String,
        record_id: String,
        #[arg(long)]
        cells: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Update many records from a JSON array of {_id, cells}. Output: {data:…}.
    BulkUpdate {
        table_id: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Delete one record (destructive). Output: {data:{message}}.
    Delete { table_id: String, record_id: String },
    /// Delete records by id (destructive). Output: {data:{message}}.
    BulkDelete {
        table_id: String,
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Set cells on every row matching --filter (destructive; shows the matching count first). Output: {data:{matching,result}}.
    #[command(after_help = FILTER_HELP)]
    UpdateByFilter {
        table_id: String,
        #[arg(long)]
        filter: String,
        #[arg(long)]
        cells: String,
        /// Allow an empty filter (every row)
        #[arg(long)]
        all_rows: bool,
    },
    /// Delete every row matching --filter (destructive; shows the matching count first). Output: {data:{matching,result}}.
    #[command(after_help = FILTER_HELP)]
    DeleteByFilter {
        table_id: String,
        #[arg(long)]
        filter: String,
        #[arg(long)]
        all_rows: bool,
    },
}

fn parse_filter(s: Option<&str>) -> Result<Value> {
    let v: Value = match s {
        Some(s) => serde_json::from_str(s)?,
        None => serde_json::json!({}),
    };
    let inner = if let Some(f) = v.get("filter") {
        f.clone()
    } else {
        v
    };
    if !inner.is_object() {
        return Err(CliError::validation("--filter must be a JSON object"));
    }
    let mut f = inner;
    if f.get("logicalOperator").is_none() && f.get("conditions").is_some() {
        f["logicalOperator"] = "and".into();
    }
    if f.get("conditions").is_none() && f.get("filterGroups").is_none() {
        f["conditions"] = serde_json::json!([]);
        f["logicalOperator"] = "and".into();
    }
    Ok(f)
}

fn parse_cells(inline: Option<&str>, file: Option<&Path>) -> Result<Value> {
    let v = read_json_arg(inline, file)?;
    if !v.is_object() {
        return Err(CliError::validation(
            "cells must be a JSON object {columnId: value}",
        ));
    }
    let only_cells =
        v.get("cells").is_some() && v.as_object().map(|o| o.len() == 1).unwrap_or(false);
    Ok(if only_cells {
        v
    } else {
        serde_json::json!({ "cells": v })
    })
}

pub async fn run(g: &Global, c: RecordsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    let base = |t: &str| format!("/v1/app-builder/table/{t}");
    match c.cmd {
        RecordsSub::Query {
            table_id,
            filter,
            q: search,
            sort_col,
            sort_dir,
            page,
            page_size,
            expand_refs,
        } => {
            let body = parse_filter(filter.as_deref())?;
            let mut pairs = page_args(page, page_size)?;
            pairs.push(("appId".into(), app.clone()));
            pairs.push(("sortCol".into(), sort_col));
            pairs.push(("sortDir".into(), sort_dir.to_string()));
            if let Some(s) = search {
                pairs.push(("q".into(), s));
            }
            if expand_refs {
                pairs.push(("fetchAllRef".into(), "true".into()));
            }
            let v = api
                .post(
                    &format!("{}/paged-record", base(&table_id)),
                    &q(&pairs),
                    &body,
                )
                .await?;
            Ok(list_from(&v, page, page_size).with_context(cx))
        }
        RecordsSub::Count { table_id, filter } => {
            let f = parse_filter(filter.as_deref())?;
            let v = api
                .post(
                    &format!("{}/record/count", base(&table_id)),
                    &app_q,
                    &serde_json::json!({ "filter": f }),
                )
                .await?;
            Ok(Output::item(v).with_context(cx))
        }
        RecordsSub::Aggregate {
            table_id,
            body,
            file,
        } => {
            let b = read_json_arg(body.as_deref(), file.as_deref())?;
            Ok(Output::item(
                api.post(&format!("{}/record/aggregate", base(&table_id)), &app_q, &b)
                    .await?,
            )
            .with_context(cx))
        }
        RecordsSub::Get {
            table_id,
            record_id,
        } => Ok(item_from(
            api.get(&format!("{}/record/{record_id}", base(&table_id)), &app_q)
                .await?,
        )
        .with_context(cx)),
        RecordsSub::GetMany { table_id, ids } => {
            let v = api
                .post(
                    &format!("{}/record-bulk-get", base(&table_id)),
                    &app_q,
                    &serde_json::json!({ "arr": ids }),
                )
                .await?;
            let data = list_items(&v);
            Ok(Output::list(data, None).with_context(cx))
        }
        RecordsSub::Create {
            table_id,
            cells,
            file,
        } => {
            let body = parse_cells(cells.as_deref(), file.as_deref())?;
            let path = format!("{}/record", base(&table_id));
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &app_q, &body).await?).with_context(cx))
        }
        RecordsSub::BulkCreate { table_id, file } => {
            let rows = read_json_arg(None, Some(&file))?;
            let arr: Vec<Value> = rows
                .as_array()
                .ok_or_else(|| CliError::validation("file must contain a JSON array"))?
                .iter()
                .map(|r| {
                    if r.get("cells").is_some() {
                        r.clone()
                    } else {
                        serde_json::json!({ "cells": r })
                    }
                })
                .collect();
            if arr.len() > 500 {
                return Err(
                    CliError::validation("at most 500 rows per call").with_hint("split the file")
                );
            }
            let path = format!("{}/record-bulk", base(&table_id));
            let body = serde_json::json!({ "arr": arr });
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &app_q, &body).await?).with_context(cx))
        }
        RecordsSub::Update {
            table_id,
            record_id,
            cells,
            file,
        } => {
            let body = parse_cells(cells.as_deref(), file.as_deref())?;
            let path = format!("{}/record/{record_id}", base(&table_id));
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        RecordsSub::BulkUpdate { table_id, file } => {
            let rows = read_json_arg(None, Some(&file))?;
            if !rows.is_array() {
                return Err(CliError::validation(
                    "file must contain a JSON array of {_id, cells}",
                ));
            }
            let path = format!("{}/record-bulk", base(&table_id));
            let body = serde_json::json!({ "arr": rows });
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        RecordsSub::Delete {
            table_id,
            record_id,
        } => {
            let path = format!("{}/record/{record_id}", base(&table_id));
            match gate.confirm(
                &format!("delete record {record_id} from table {table_id}"),
                &cx,
            )? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("record {record_id} deleted")).with_context(cx))
                }
            }
        }
        RecordsSub::BulkDelete { table_id, ids } => {
            let path = format!("{}/record", base(&table_id));
            let body = Value::Array(ids.iter().map(|s| Value::String(s.clone())).collect());
            match gate.confirm(
                &format!("delete {} records from table {table_id}", ids.len()),
                &cx,
            )? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, Some(&body), cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, Some(&body)).await?;
                    Ok(Output::message(format!("{} records deleted", ids.len())).with_context(cx))
                }
            }
        }
        RecordsSub::UpdateByFilter {
            table_id,
            filter,
            cells,
            all_rows,
        } => {
            let f = parse_filter(Some(&filter))?;
            require_filter(&f, all_rows)?;
            let cells: Value = serde_json::from_str(&cells)?;
            let matching = count(&api, &base(&table_id), &app_q, &f).await?;
            let path = format!("{}/record-bulk-update-by-filter", base(&table_id));
            let body = serde_json::json!({ "filter": f, "cells": cells });
            match gate.confirm(
                &format!("update {matching} matching records in table {table_id}"),
                &cx,
            )? {
                Decision::DryRun => {
                    let mut r = dry_run_plan("PUT", &path, Some(&body), cx);
                    with_matching(&mut r, matching);
                    Ok(r)
                }
                Decision::Proceed => {
                    let v = api.put(&path, &app_q, &body).await?;
                    Ok(
                        Output::item(serde_json::json!({ "matching": matching, "result": v }))
                            .with_context(cx),
                    )
                }
            }
        }
        RecordsSub::DeleteByFilter {
            table_id,
            filter,
            all_rows,
        } => {
            let f = parse_filter(Some(&filter))?;
            require_filter(&f, all_rows)?;
            let matching = count(&api, &base(&table_id), &app_q, &f).await?;
            let path = format!("{}/record-bulk-delete-by-filter", base(&table_id));
            let body = serde_json::json!({ "filter": f });
            match gate.confirm(
                &format!("delete {matching} matching records from table {table_id}"),
                &cx,
            )? {
                Decision::DryRun => {
                    let mut r = dry_run_plan("POST", &path, Some(&body), cx);
                    with_matching(&mut r, matching);
                    Ok(r)
                }
                Decision::Proceed => {
                    let v = api.post(&path, &app_q, &body).await?;
                    Ok(
                        Output::item(serde_json::json!({ "matching": matching, "result": v }))
                            .with_context(cx),
                    )
                }
            }
        }
    }
}

async fn count(api: &ApiClient, base: &str, app_q: &[(&str, &str)], filter: &Value) -> Result<u64> {
    let v = api
        .post(
            &format!("{base}/record/count"),
            app_q,
            &serde_json::json!({ "filter": filter }),
        )
        .await?;
    Ok(v.get("count").and_then(Value::as_u64).unwrap_or(0))
}

fn with_matching(r: &mut Rendered, matching: u64) {
    if let crate::output::Output::Item(v) = &mut r.output {
        v["matching"] = serde_json::json!(matching);
    }
}
