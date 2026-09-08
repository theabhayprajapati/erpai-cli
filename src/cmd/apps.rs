use super::*;
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct AppsCmd {
    #[command(subcommand)]
    pub cmd: AppsSub,
}

#[derive(Subcommand, Debug)]
pub enum AppsSub {
    /// List apps. Output: {data:[{_id,name,description,…}], page:{no,size,total}}. Errors: auth_error, network_error, api_error.
    List {
        /// Search text
        #[arg(long)]
        q: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 30)]
        page_size: u32,
    },
    /// Get one app. Output: {data:{_id,name,…}}. Errors: not_found, forbidden, auth_error.
    Get { app_id: String },
}

pub async fn run(g: &Global, c: AppsCmd) -> Result<Rendered> {
    let (p, api) = ctx(g)?;
    match c.cmd {
        AppsSub::List {
            q: search,
            page,
            page_size,
        } => {
            let mut pairs = page_args(page, page_size)?;
            pairs.push(("sortCol".into(), "UTDT".into()));
            pairs.push(("sortDir".into(), "-1".into()));
            if let Some(s) = search {
                pairs.push(("q".into(), s));
            }
            let v = api.get("/v1/app-builder/app", &q(&pairs)).await?;
            Ok(list_from(&v, page, page_size).with_context(context(&p, None)))
        }
        AppsSub::Get { app_id } => {
            crate::safety::preflight_app(&p, &app_id)?;
            let v = api
                .get(&format!("/v1/app-builder/app/{app_id}"), &[])
                .await?;
            Ok(item_from(v).with_context(context(&p, Some(&app_id))))
        }
    }
}
