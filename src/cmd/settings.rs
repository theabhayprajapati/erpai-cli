use crate::cli::Global;
use crate::config::{ProfileStore, DEFAULT_BASE_URL};
use crate::error::Result;
use crate::output::{Output, Rendered};

pub fn run(g: &Global) -> Result<Rendered> {
    let store = ProfileStore::open()?;
    let profiles = store.list()?;
    let active = store.load(&g.profile)?.map(|p| p.identity());
    Ok(Output::item(serde_json::json!({
        "configDir": store.dir().display().to_string(),
        "profiles": profiles,
        "activeProfile": g.profile,
        "active": active,
        "defaultBaseUrl": DEFAULT_BASE_URL,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}
