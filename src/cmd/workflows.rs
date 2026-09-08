use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

const BASE: &str = "/v1/auto-builder";

#[derive(Args, Debug)]
pub struct WorkflowsCmd {
    #[command(subcommand)]
    pub cmd: WorkflowsSub,
}

#[derive(Subcommand, Debug)]
pub enum WorkflowsSub {
    /// List workflows of --app. Output: {data:[{_id,name,active,…}], page}.
    List {
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 50)]
        page_size: u32,
    },
    /// Get a workflow with nodes and connections. Output: {data:{_id,name,nodes,connections,active,…}}.
    Get { workflow_id: String },
    /// Create a workflow from a full graph JSON {name,nodes,connections,settings?}. Output: {data:{_id,…}}. Errors: validation_error (missing name/nodes/connections).
    Create {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Replace the WHOLE graph (PUT). Nodes missing from the body are deleted — prefer patch-node for one node. Output: {data:{…}}.
    Update {
        workflow_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Update one node with dotted-path sets, e.g. --set '{"parameters.customActionName":"<action-id>"}'. Output: {data:{…}}.
    PatchNode {
        workflow_id: String,
        node_id: String,
        #[arg(long)]
        set: String,
    },
    /// Rename a workflow. Output: {data:{…}}.
    Rename {
        workflow_id: String,
        #[arg(long)]
        name: String,
    },
    /// Delete a workflow (destructive). Output: {data:{message}}.
    Delete { workflow_id: String },
    /// Start listening for triggers. Output: {data:{…}}.
    Activate { workflow_id: String },
    /// Stop listening for triggers. Output: {data:{…}}.
    Deactivate { workflow_id: String },
    /// Run the workflow for real (side effects happen). Output: {data:{executionId?,…}}.
    Execute {
        workflow_id: String,
        /// Trigger input JSON (default {})
        #[arg(long)]
        input: Option<String>,
    },
    /// Run one node with the given upstream item as input (real call, side effects happen). Output: {data:{success,outputData,…}}.
    TestNode {
        workflow_id: String,
        node_id: String,
        #[arg(long)]
        input: String,
    },
    /// List executions of a workflow. Output: {data:[{_id,status,startedAt,…}]}.
    Executions {
        workflow_id: String,
        #[arg(long, default_value_t = 10)]
        limit: u32,
        /// Filter by status, e.g. error, success, running
        #[arg(long)]
        status: Option<String>,
    },
    /// Inspect one execution; --nodes for per-node status, --final-output for the last node's output. Output: {data:{…}}.
    Execution {
        execution_id: String,
        #[arg(long, conflicts_with = "final_output")]
        nodes: bool,
        #[arg(long)]
        final_output: bool,
    },
    /// Stop a running execution. Output: {data:{…}}.
    ExecutionStop { execution_id: String },
    /// Retry a failed execution. Output: {data:{…}}.
    ExecutionRetry { execution_id: String },
    /// Per-node counts and timings for a run. Output: {data:{…}}.
    RunSummary {
        workflow_id: String,
        run_ref: String,
    },
    /// One page of a node's input/output items for a run (page is 1-based). Output: {data:{…}}.
    RunNode {
        workflow_id: String,
        run_ref: String,
        node_id: String,
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
    /// Node catalog
    Nodes {
        #[command(subcommand)]
        cmd: NodesSub,
    },
    /// Credentials used by workflow nodes
    Credentials {
        #[command(subcommand)]
        cmd: CredentialsSub,
    },
}

#[derive(Subcommand, Debug)]
pub enum NodesSub {
    /// List node types (summaries). Output: {data:[{name,displayName,category,…}]}.
    List {
        /// e.g. trigger, action, integration
        #[arg(long)]
        category: Option<String>,
    },
    /// Full node definition: properties, inputs, outputs, credentials. Output: {data:{…}}.
    Schema { node_type: String },
    /// Resolve a dynamic dropdown; body carries the parameters the method depends on. Output: {data:[{name,value}]} — store `value`.
    Options {
        node_type: String,
        param: String,
        #[arg(long, default_value = "{}")]
        body: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum CredentialsSub {
    /// List credentials visible to --app. Output: {data:[{_id,name,type,…}]} (secrets are never returned).
    List,
    /// Get one credential's metadata. Output: {data:{_id,name,type,…}}.
    Get { credential_id: String },
    /// Create a credential {name,type,data:{…}}. Output: {data:{_id,…}}.
    Create {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Update a credential. Output: {data:{…}}.
    Update {
        credential_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Delete a credential (destructive; workflows using it will fail). Output: {data:{message}}.
    Delete { credential_id: String },
    /// Test a credential against its provider (real call). Output: {data:{success,…}}.
    Test { credential_id: String },
    /// Credential type definitions. Output: {data:[…]}.
    Types,
}

fn require_graph(body: &Value) -> Result<()> {
    let ok = body
        .get("name")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        && body.get("nodes").map(Value::is_array).unwrap_or(false)
        && body
            .get("connections")
            .map(Value::is_object)
            .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(CliError::validation("workflow body needs name, nodes (array) and connections (object)")
            .with_hint("discover node types with `erpai workflows nodes list`; the build-workflows skill describes the graph shape"))
    }
}

fn list_any(v: &Value) -> Vec<Value> {
    v.as_array()
        .cloned()
        .or_else(|| v.get("data").and_then(Value::as_array).cloned())
        .or_else(|| v.get("body").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

pub async fn run(g: &Global, c: WorkflowsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        WorkflowsSub::List { page, page_size } => {
            let mut pairs = page_args(page, page_size)?;
            pairs.push(("appId".into(), app.clone()));
            let v = api.get(&format!("{BASE}/workflows"), &q(&pairs)).await?;
            Ok(list_from(&v, page, page_size).with_context(cx))
        }
        WorkflowsSub::Get { workflow_id } => Ok(item_from(
            api.get(&format!("{BASE}/workflows/{workflow_id}"), &[])
                .await?,
        )
        .with_context(cx)),
        WorkflowsSub::Create { body, file } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            require_graph(&body)?;
            let path = format!("{BASE}/workflows");
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &app_q, &body).await?).with_context(cx))
        }
        WorkflowsSub::Update {
            workflow_id,
            body,
            file,
        } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            require_graph(&body)?;
            let path = format!("{BASE}/workflows/{workflow_id}");
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &[], &body).await?).with_context(cx))
        }
        WorkflowsSub::PatchNode {
            workflow_id,
            node_id,
            set,
        } => {
            let set: Value = serde_json::from_str(&set)?;
            if !set.is_object() {
                return Err(CliError::validation(
                    "--set must be a JSON object of dotted paths to values",
                ));
            }
            let path = format!("{BASE}/workflows/{workflow_id}/nodes/{node_id}");
            let body = serde_json::json!({ "set": set });
            if gate.dry_run {
                return Ok(dry_run_plan("PATCH", &path, Some(&body), cx));
            }
            Ok(item_from(api.patch(&path, &[], &body).await?).with_context(cx))
        }
        WorkflowsSub::Rename { workflow_id, name } => {
            let path = format!("{BASE}/workflows/{workflow_id}/name");
            let body = serde_json::json!({ "name": name });
            if gate.dry_run {
                return Ok(dry_run_plan("PATCH", &path, Some(&body), cx));
            }
            Ok(item_from(api.patch(&path, &[], &body).await?).with_context(cx))
        }
        WorkflowsSub::Delete { workflow_id } => {
            let path = format!("{BASE}/workflows/{workflow_id}");
            match gate.confirm(&format!("delete workflow {workflow_id}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &[], None).await?;
                    Ok(Output::message(format!("workflow {workflow_id} deleted")).with_context(cx))
                }
            }
        }
        WorkflowsSub::Activate { workflow_id } => {
            post_or_plan(
                &api,
                gate,
                &format!("{BASE}/workflows/{workflow_id}/activate"),
                &serde_json::json!({}),
                cx,
            )
            .await
        }
        WorkflowsSub::Deactivate { workflow_id } => {
            post_or_plan(
                &api,
                gate,
                &format!("{BASE}/workflows/{workflow_id}/deactivate"),
                &serde_json::json!({}),
                cx,
            )
            .await
        }
        WorkflowsSub::Execute { workflow_id, input } => {
            let body: Value = match input {
                Some(s) => serde_json::from_str(&s)?,
                None => serde_json::json!({}),
            };
            post_or_plan(
                &api,
                gate,
                &format!("{BASE}/workflows/{workflow_id}/execute"),
                &body,
                cx,
            )
            .await
        }
        WorkflowsSub::TestNode {
            workflow_id,
            node_id,
            input,
        } => {
            let body: Value = serde_json::from_str(&input)?;
            post_or_plan(
                &api,
                gate,
                &format!("{BASE}/workflows/{workflow_id}/nodes/{node_id}/test-execute"),
                &body,
                cx,
            )
            .await
        }
        WorkflowsSub::Executions {
            workflow_id,
            limit,
            status,
        } => {
            let mut pairs = vec![
                ("page".to_string(), "1".to_string()),
                ("limit".to_string(), limit.to_string()),
            ];
            if let Some(s) = status {
                pairs.push(("status".into(), s));
            }
            let v = api
                .get(
                    &format!("{BASE}/executions/workflow/{workflow_id}"),
                    &q(&pairs),
                )
                .await?;
            Ok(Output::list(list_any(&v), None).with_context(cx))
        }
        WorkflowsSub::Execution {
            execution_id,
            nodes,
            final_output,
        } => {
            let suffix = if nodes {
                "/nodes"
            } else if final_output {
                "/final-output"
            } else {
                ""
            };
            Ok(item_from(
                api.get(&format!("{BASE}/executions/{execution_id}{suffix}"), &[])
                    .await?,
            )
            .with_context(cx))
        }
        WorkflowsSub::ExecutionStop { execution_id } => {
            post_or_plan(
                &api,
                gate,
                &format!("{BASE}/executions/{execution_id}/stop"),
                &serde_json::json!({}),
                cx,
            )
            .await
        }
        WorkflowsSub::ExecutionRetry { execution_id } => {
            post_or_plan(
                &api,
                gate,
                &format!("{BASE}/executions/{execution_id}/retry"),
                &serde_json::json!({}),
                cx,
            )
            .await
        }
        WorkflowsSub::RunSummary {
            workflow_id,
            run_ref,
        } => Ok(item_from(
            api.get(
                &format!("{BASE}/workflows/{workflow_id}/runs/{run_ref}/summary"),
                &[],
            )
            .await?,
        )
        .with_context(cx)),
        WorkflowsSub::RunNode {
            workflow_id,
            run_ref,
            node_id,
            page,
        } => {
            let pg = page.to_string();
            Ok(item_from(
                api.get(
                    &format!("{BASE}/workflows/{workflow_id}/runs/{run_ref}/nodes/{node_id}"),
                    &[("page", pg.as_str())],
                )
                .await?,
            )
            .with_context(cx))
        }
        WorkflowsSub::Nodes { cmd } => match cmd {
            NodesSub::List { category } => {
                let pairs: Vec<(String, String)> = category
                    .map(|c| vec![("category".to_string(), c)])
                    .unwrap_or_default();
                let v = api.get(&format!("{BASE}/nodes"), &q(&pairs)).await?;
                Ok(Output::list(list_any(&v), None).with_context(cx))
            }
            NodesSub::Schema { node_type } => Ok(item_from(
                api.get(&format!("{BASE}/nodes/{node_type}"), &[]).await?,
            )
            .with_context(cx)),
            NodesSub::Options {
                node_type,
                param,
                body,
            } => {
                let body: Value = serde_json::from_str(&body)?;
                let v = api
                    .post(
                        &format!("{BASE}/nodes/{node_type}/parameters/{param}/options"),
                        &[],
                        &body,
                    )
                    .await?;
                Ok(Output::list(list_any(&v), None).with_context(cx))
            }
        },
        WorkflowsSub::Credentials { cmd } => match cmd {
            CredentialsSub::List => {
                let v = api.get(&format!("{BASE}/credentials"), &app_q).await?;
                Ok(Output::list(list_any(&v), None).with_context(cx))
            }
            CredentialsSub::Get { credential_id } => Ok(item_from(
                api.get(&format!("{BASE}/credentials/{credential_id}"), &[])
                    .await?,
            )
            .with_context(cx)),
            CredentialsSub::Create { body, file } => {
                let body = read_json_arg(body.as_deref(), file.as_deref())?;
                post_or_plan(&api, gate, &format!("{BASE}/credentials"), &body, cx).await
            }
            CredentialsSub::Update {
                credential_id,
                body,
                file,
            } => {
                let body = read_json_arg(body.as_deref(), file.as_deref())?;
                let path = format!("{BASE}/credentials/{credential_id}");
                if gate.dry_run {
                    return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
                }
                Ok(item_from(api.put(&path, &[], &body).await?).with_context(cx))
            }
            CredentialsSub::Delete { credential_id } => {
                let path = format!("{BASE}/credentials/{credential_id}");
                match gate.confirm(
                    &format!("delete credential {credential_id} (workflows using it will fail)"),
                    &cx,
                )? {
                    Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                    Decision::Proceed => {
                        api.delete(&path, &[], None).await?;
                        Ok(
                            Output::message(format!("credential {credential_id} deleted"))
                                .with_context(cx),
                        )
                    }
                }
            }
            CredentialsSub::Test { credential_id } => {
                post_or_plan(
                    &api,
                    gate,
                    &format!("{BASE}/credentials/{credential_id}/test"),
                    &serde_json::json!({}),
                    cx,
                )
                .await
            }
            CredentialsSub::Types => {
                let v = api.get(&format!("{BASE}/credentials/types"), &[]).await?;
                Ok(Output::list(list_any(&v), None).with_context(cx))
            }
        },
    }
}

async fn post_or_plan(
    api: &ApiClient,
    gate: Gate,
    path: &str,
    body: &Value,
    cx: Context,
) -> Result<Rendered> {
    if gate.dry_run {
        return Ok(dry_run_plan("POST", path, Some(body), cx));
    }
    Ok(item_from(api.post(path, &[], body).await?).with_context(cx))
}
