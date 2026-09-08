use crate::cli::Global;
use crate::error::{CliError, Result};
use crate::output::Rendered;

pub fn run(_g: &Global) -> Result<Rendered> {
    Err(CliError::auth("not signed in").with_hint("run `erpai login`"))
}
