use super::*;
use crate::safety::{preflight_app, resolve_app, Gate};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct AttachmentsCmd {
    #[command(subcommand)]
    pub cmd: AttachmentsSub,
}

#[derive(Subcommand, Debug)]
pub enum AttachmentsSub {
    /// Upload a private record attachment. Output: {data:{path, attachment:{path,url,title,type,fileSize,extension,isProtected}}} — `attachment` is the ready-made file-cell value.
    Upload { file: PathBuf },
    /// Resolve a protected attachment path to a temporary download URL. Output: {data:{…}}.
    DownloadUrl {
        #[arg(long)]
        path: String,
    },
}

pub async fn run(g: &Global, c: AttachmentsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    let app = resolve_app(g)?;
    preflight_app(&p, &app)?;
    let cx = context(&p, Some(&app));
    let gate = Gate::from(g);
    let app_q = [("appId", app.as_str())];
    match c.cmd {
        AttachmentsSub::Upload { file } => {
            let bytes = std::fs::read(&file)?;
            let name = file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upload")
                .to_string();
            let mime = mime_guess::from_path(&file)
                .first_or_octet_stream()
                .to_string();
            let ext = file
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let size = bytes.len();
            if gate.dry_run {
                return Ok(dry_run_plan(
                    "POST",
                    "/v1/attachment/file-upload",
                    Some(
                        &serde_json::json!({ "multipart": { "file": name, "file-entity": "APP_BUILDER_ATTACHMENT_RECORD", "bytes": size } }),
                    ),
                    cx,
                ));
            }
            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(name.clone())
                .mime_str(&mime)
                .map_err(|e| CliError::internal(e.to_string()))?;
            let form = reqwest::multipart::Form::new()
                .part("file", part)
                .text("file-entity", "APP_BUILDER_ATTACHMENT_RECORD");
            let v = api
                .post_multipart("/v1/attachment/file-upload", &app_q, form)
                .await?;
            let path = ["relativePath", "path", "url"]
                .iter()
                .find_map(|k| v.get(k).and_then(Value::as_str))
                .or_else(|| v.pointer("/data/path").and_then(Value::as_str))
                .ok_or_else(|| CliError::api(200, "upload response carried no path"))?
                .to_string();
            let attachment = serde_json::json!({
                "path": path, "url": path, "title": name, "type": mime, "fileSize": size,
                "extension": ext, "isProtected": true,
            });
            Ok(Output::item(
                serde_json::json!({ "path": path, "attachment": attachment, "raw": v }),
            )
            .with_context(cx))
        }
        AttachmentsSub::DownloadUrl { path } => {
            let v = api
                .get(
                    "/v1/attachment/file-download",
                    &[("appId", app.as_str()), ("file", path.as_str())],
                )
                .await?;
            Ok(Output::item(v).with_context(cx))
        }
    }
}
