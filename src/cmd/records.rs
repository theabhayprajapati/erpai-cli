use crate::cli::Global;
use crate::error::{CliError, Result};
use crate::output::Rendered;

#[derive(clap::Args, Debug)]
#[allow(dead_code)]
pub struct RecordsCmd {
    /// (stub) accepts anything until the real command tree lands
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    rest: Vec<String>,
}

pub async fn run(_g: &Global, _c: RecordsCmd) -> Result<Rendered> {
    Err(CliError::auth("not signed in").with_hint("run `erpai login`"))
}
