use crate::output::Format;
use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "erpai",
    version,
    about = "ERP AI command-line client for coding agents",
    long_about = "ERP AI command-line client. Every command prints JSON on stdout ({\"data\": …}) and, on failure, a JSON error on stderr ({\"error\": {code, message, hint?, requestId?}}) with a categorical exit code: 1 internal · 2 validation · 3 auth · 4 forbidden · 5 network · 6 api · 7 not_found. Run `erpai <group> <command> --help` for each command's output shape."
)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args, Debug, Clone)]
pub struct Global {
    /// Credential profile (environment). Default: "default".
    #[arg(long, global = true, env = "ERPAI_PROFILE", default_value = "default")]
    pub profile: String,
    /// App id for app-scoped commands. Also ERPAI_APP_ID or a `.erpai/app` file in the working directory.
    #[arg(long, global = true, env = "ERPAI_APP_ID")]
    pub app: Option<String>,
    /// Output format.
    #[arg(long, global = true, value_enum, default_value_t = Format::Json)]
    pub format: Format,
    /// Skip interactive confirmation of destructive commands.
    #[arg(long, global = true)]
    pub yes: bool,
    /// Validate and print the request plan without sending any mutating request.
    #[arg(long, global = true)]
    pub dry_run: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Apps in the current organisation
    Apps(crate::cmd::apps::AppsCmd),
    /// Tables of an app (--app)
    Tables(crate::cmd::tables::TablesCmd),
    /// Columns of a table (--app)
    Columns(crate::cmd::columns::ColumnsCmd),
    /// Records of a table (--app)
    Records(crate::cmd::records::RecordsCmd),
    /// SQL over an app's data (--app)
    Sql(crate::cmd::sql::SqlCmd),
    /// Workflows (automations) of an app (--app)
    Workflows(crate::cmd::workflows::WorkflowsCmd),
    /// Saved views of a table (--app, --table)
    Layouts(crate::cmd::layouts::LayoutsCmd),
    /// Entry forms of tables (--app, --table)
    Forms(crate::cmd::forms::FormsCmd),
    /// Rich-text documents and folders (--app)
    Documents(crate::cmd::documents::DocumentsCmd),
    /// Roles, user access, invitations (--app)
    Roles(crate::cmd::roles::RolesCmd),
    /// Custom HTML pages and the home page (--app)
    Pages(crate::cmd::pages::PagesCmd),
    /// Insights widgets above tables (--app, --table)
    Widgets(crate::cmd::widgets::WidgetsCmd),
    /// Public template catalog publishing (--app)
    Catalog(crate::cmd::catalog::CatalogCmd),
    /// Private record attachments (--app)
    Attachments(crate::cmd::attachments::AttachmentsCmd),
    /// Generic request to a public endpoint without a dedicated command. Output: {data:…}. GET is read-only; POST/PUT/PATCH honour --dry-run; DELETE is destructive-gated.
    Api(crate::cmd::api::ApiCmd),
    /// Sign in (browser) or store an API key
    Login(crate::auth::LoginArgs),
    /// Revoke the stored key and forget the profile
    Logout,
    /// Show who the current profile is authenticated as
    Whoami,
    /// Check the runtime: profile, base URL, connectivity, key validity. Exit code is that of the first failing check.
    Doctor,
    /// Check whether a newer CLI release exists (never changes anything itself)
    Update(crate::cmd::update::UpdateArgs),
    /// Show effective settings (profiles, paths)
    Settings,
}
