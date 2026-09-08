use super::*;
use crate::safety::{preflight_app, resolve_app, Decision, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct DocumentsCmd {
    #[command(subcommand)]
    pub cmd: DocumentsSub,
}

#[derive(Subcommand, Debug)]
pub enum DocumentsSub {
    /// List documents. Output: {data:[{_id,name,emoji,…}], page}.
    List {
        #[arg(long)]
        q: Option<String>,
        #[arg(long)]
        folder: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 30)]
        page_size: u32,
    },
    /// Get a document with its content (BlockNote JSON). Output: {data:{_id,name,content,…}}.
    Get { document_id: String },
    /// Create a document. --content-file holds a BlockNote block array. Output: {data:{_id,…}}.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        emoji: Option<String>,
        #[arg(long)]
        folder: Option<String>,
        #[arg(long)]
        content_file: Option<PathBuf>,
        #[arg(long)]
        draft: bool,
    },
    /// Update name/content/emoji/folder from a JSON body. Output: {data:{…}}.
    Update {
        document_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Delete a document (destructive). Output: {data:{message}}.
    Delete { document_id: String },
    /// Duplicate a document. Output: {data:{_id,…}}.
    Duplicate {
        document_id: String,
        #[arg(long)]
        title: String,
    },
    /// Document folders (flat)
    Folders {
        #[command(subcommand)]
        cmd: FoldersSub,
    },
}

#[derive(Subcommand, Debug)]
pub enum FoldersSub {
    /// List folders. Output: {data:[{_id,name}]}.
    List,
    /// Create a folder. Output: {data:{_id,name}}.
    Create {
        #[arg(long)]
        name: String,
    },
    /// Rename a folder. Output: {data:{…}}.
    Update {
        folder_id: String,
        #[arg(long)]
        name: String,
    },
    /// Delete a folder (destructive; its documents remain). Output: {data:{message}}.
    Delete { folder_id: String },
}

pub async fn run(g: &Global, c: DocumentsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        DocumentsSub::List {
            q: search,
            folder,
            page,
            page_size,
        } => {
            // this endpoint alone pages from 0; the CLI keeps 1-based pages everywhere
            let mut pairs = page_args(page, page_size)?;
            for (k, v) in pairs.iter_mut() {
                if k == "pageNo" {
                    *v = page.saturating_sub(1).to_string();
                }
            }
            pairs.push(("appId".into(), app.clone()));
            if let Some(s) = search {
                pairs.push(("q".into(), s));
            }
            if let Some(f) = folder {
                pairs.push(("folderId".into(), f));
            }
            let v = api.get("/v1/app-builder/app-document", &q(&pairs)).await?;
            Ok(list_from(&v, page, page_size).with_context(cx))
        }
        DocumentsSub::Get { document_id } => Ok(item_from(
            api.get(
                &format!("/v1/app-builder/app-document/{document_id}"),
                &app_q,
            )
            .await?,
        )
        .with_context(cx)),
        DocumentsSub::Create {
            name,
            emoji,
            folder,
            content_file,
            draft,
        } => {
            let content = match content_file {
                Some(f) => {
                    let v = read_json_arg(None, Some(&f))?;
                    if !v.is_array() {
                        return Err(CliError::validation(
                            "--content-file must contain a JSON array of BlockNote blocks",
                        ));
                    }
                    v
                }
                None => serde_json::json!([]),
            };
            let mut body = serde_json::json!({ "title": name, "content": content, "isDraft": draft, "appId": app });
            if let Some(e) = emoji {
                body["emoji"] = e.into();
            }
            if let Some(f) = folder {
                body["folderId"] = f.into();
            }
            if gate.dry_run {
                return Ok(dry_run_plan(
                    "POST",
                    "/v1/app-builder/app-document",
                    Some(&body),
                    cx,
                ));
            }
            Ok(item_from(
                api.post("/v1/app-builder/app-document", &app_q, &body)
                    .await?,
            )
            .with_context(cx))
        }
        DocumentsSub::Update {
            document_id,
            body,
            file,
        } => {
            let body = read_json_arg(body.as_deref(), file.as_deref())?;
            let path = format!("/v1/app-builder/app-document/{document_id}");
            if gate.dry_run {
                return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
            }
            Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
        }
        DocumentsSub::Delete { document_id } => {
            let path = format!("/v1/app-builder/app-document/{document_id}");
            match gate.confirm(&format!("delete document {document_id}"), &cx)? {
                Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                Decision::Proceed => {
                    api.delete(&path, &app_q, None).await?;
                    Ok(Output::message(format!("document {document_id} deleted")).with_context(cx))
                }
            }
        }
        DocumentsSub::Duplicate { document_id, title } => {
            let path = format!("/v1/app-builder/app-document/{document_id}/duplicate");
            let body = serde_json::json!({ "title": title });
            if gate.dry_run {
                return Ok(dry_run_plan("POST", &path, Some(&body), cx));
            }
            Ok(item_from(api.post(&path, &app_q, &body).await?).with_context(cx))
        }
        DocumentsSub::Folders { cmd } => match cmd {
            FoldersSub::List => {
                let v = api
                    .get("/v1/app-builder/app-document-folder", &app_q)
                    .await?;
                let data = list_items(&v);
                Ok(Output::list(data, None).with_context(cx))
            }
            FoldersSub::Create { name } => {
                let body = serde_json::json!({ "name": name });
                if gate.dry_run {
                    return Ok(dry_run_plan(
                        "POST",
                        "/v1/app-builder/app-document-folder",
                        Some(&body),
                        cx,
                    ));
                }
                Ok(item_from(
                    api.post("/v1/app-builder/app-document-folder", &app_q, &body)
                        .await?,
                )
                .with_context(cx))
            }
            FoldersSub::Update { folder_id, name } => {
                let path = format!("/v1/app-builder/app-document-folder/{folder_id}");
                let body = serde_json::json!({ "name": name });
                if gate.dry_run {
                    return Ok(dry_run_plan("PUT", &path, Some(&body), cx));
                }
                Ok(item_from(api.put(&path, &app_q, &body).await?).with_context(cx))
            }
            FoldersSub::Delete { folder_id } => {
                let path = format!("/v1/app-builder/app-document-folder/{folder_id}");
                match gate.confirm(&format!("delete folder {folder_id}"), &cx)? {
                    Decision::DryRun => Ok(dry_run_plan("DELETE", &path, None, cx)),
                    Decision::Proceed => {
                        api.delete(&path, &app_q, None).await?;
                        Ok(Output::message(format!("folder {folder_id} deleted")).with_context(cx))
                    }
                }
            }
        },
    }
}
