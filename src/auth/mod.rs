use crate::cli::Global;
use crate::error::{CliError, Result};
use crate::output::Rendered;

#[derive(clap::Args, Debug)]
pub struct LoginArgs {}

fn stub() -> Result<Rendered> {
    Err(CliError::auth("not signed in").with_hint("run `erpai login`"))
}
pub async fn login(_g: &Global, _a: LoginArgs) -> Result<Rendered> {
    stub()
}
pub async fn logout(_g: &Global) -> Result<Rendered> {
    stub()
}
pub async fn whoami(_g: &Global) -> Result<Rendered> {
    stub()
}
pub async fn doctor(_g: &Global) -> Result<Rendered> {
    stub()
}
