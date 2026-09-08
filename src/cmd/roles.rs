use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

const BASE: &str = "/v1/permission";

#[derive(Args, Debug)]
pub struct RolesCmd {
    #[command(subcommand)]
    pub cmd: RolesSub,
}

#[derive(Subcommand, Debug)]
pub enum RolesSub {
    /// List roles of --app. Output: {data:[{_id,name,code,permissions}]}.
    List,
    /// Create a role {name,code?,description?,permissions:{data:[{subModule:<table-id>,permissions:[read,create,update,delete]}]}}; moduleRef is filled from --app. Output: {data:{_id,…}}.
    Create {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Update a role. Output: {data:{…}}.
    Update {
        role_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Delete a role (destructive; users lose its grants). Output: {data:{message}}.
    Delete { role_id: String },
    /// Users of --app with their roles. Output: {data:[{iamUserId,email,roles}]}.
    Users,
    /// Assign roles to a user (destructive: changes access). Output: {data:{…}}.
    Assign {
        #[arg(long)]
        user: String,
        #[arg(long, required = true)]
        role: Vec<String>,
    },
    /// Remove a user from the app (destructive). Output: {data:{…}}.
    Remove {
        #[arg(long)]
        user: String,
    },
    /// Invite someone by email with roles (sends an email; destructive-gated). Output: {data:{…}}.
    Invite {
        #[arg(long)]
        email: String,
        #[arg(long, required = true)]
        role: Vec<String>,
        #[arg(long)]
        message: Option<String>,
    },
    /// Pending invitations. Output: {data:[{_id,email,status}]}.
    Invites,
    /// Cancel a pending invitation (destructive). Output: {data:{message}}.
    InviteCancel { invite_id: String },
}

fn list_any(v: &Value) -> Vec<Value> {
    v.get("data")
        .or(v.get("body"))
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default()
}

pub async fn run(g: &Global, c: RolesCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let module_q = [("moduleRef", app.as_str())];
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        RolesSub::List => {
            let v = api.get(&format!("{BASE}/role"), &module_q).await?;
            Ok(Output::list(list_any(&v), None).with_context(cx))
        }
        RolesSub::Create { body, file } => {
            let mut body = read_json_arg(body.as_deref(), file.as_deref())?;
            if body.get("moduleRef").is_none() {
                body["moduleRef"] = app.clone().into();
            }
            if body.get("name").is_none() || body.pointer("/permissions/data").is_none() {
                return Err(CliError::validation(
                    "role body needs name and permissions.data",
                ));
            }
            let path = format!("{BASE}/role");
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
        }
        RolesSub::Update {
            role_id,
            body,
            file,
        } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            let path = format!("{BASE}/role/{role_id}");
            if gate.dry_run {
                return Ok(dry_run_plan("PATCH", &path, Some(&body), cx));
            }
            Ok(item_from(api.patch(&path, &[], &body).await?).with_context(cx))
        }
        RolesSub::Delete { role_id } => {
            let path = format!("{BASE}/role/{role_id}");
            match gate.confirm(&format!("delete role {role_id}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &[], None).await?;
                    Ok(Output::message(format!("role {role_id} deleted")).with_context(cx))
                }
            }
        }
        RolesSub::Users => {
            let v = api
                .get(&format!("{BASE}/user-role-mapping/app-users"), &module_q)
                .await?;
            Ok(Output::list(list_any(&v), None).with_context(cx))
        }
        RolesSub::Assign { user, role } => {
            let path = format!("{BASE}/user-role-mapping");
            let body = serde_json::json!({ "module": "APP", "moduleRef": app, "data": [{ "iamUserId": user, "roleIds": role }] });
            match gate.confirm(
                &format!("assign roles {} to user {user}", role.join(",")),
                &cx,
            )? {
                Decision::DryRun => Ok(dry_run_plan("PATCH", &path, Some(&body), cx)),
                Decision::Proceed => {
                    Ok(item_from(api.patch(&path, &[], &body).await?).with_context(cx))
                }
            }
        }
        RolesSub::Remove { user } => {
            let path = format!("{BASE}/user-role-mapping");
            let body = serde_json::json!({ "module": "APP", "moduleRef": app, "data": [{ "iamUserId": user }] });
            match gate.confirm(&format!("remove user {user} from the app"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, Some(&body), cx)),
                Decision::Proceed => {
                    Ok(item_from(api.delete(&path, &[], Some(&body)).await?).with_context(cx))
                }
            }
        }
        RolesSub::Invite {
            email,
            role,
            message,
        } => {
            let path = format!("{BASE}/user-invite/invite-to-org");
            let mut invite = serde_json::json!({ "email": email, "roleIds": role });
            if let Some(m) = message {
                invite["message"] = m.into();
            }
            let body = serde_json::json!({ "appId": app, "invites": [invite] });
            match gate.confirm(&format!("send an invitation email to {email}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("POST", &path, Some(&body), cx)),
                Decision::Proceed => {
                    Ok(item_from(api.post(&path, &[], &body).await?).with_context(cx))
                }
            }
        }
        RolesSub::Invites => {
            let v = api.get(&format!("{BASE}/user-invite"), &app_q).await?;
            Ok(Output::list(list_any(&v), None).with_context(cx))
        }
        RolesSub::InviteCancel { invite_id } => {
            let path = format!("{BASE}/user-invite/{invite_id}");
            match gate.confirm(&format!("cancel invitation {invite_id}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("invitation {invite_id} cancelled"))
                        .with_context(cx))
                }
            }
        }
    }
}
