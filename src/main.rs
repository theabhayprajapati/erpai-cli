use clap::Parser;
use erpai::cli::{Cli, Command};
use erpai::error::CliError;
use erpai::{auth, cmd, output};

fn main() {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            use clap::error::ErrorKind::{DisplayHelp, DisplayVersion};
            if matches!(e.kind(), DisplayHelp | DisplayVersion) {
                print!("{e}");
                std::process::exit(0);
            }
            let msg = e
                .to_string()
                .lines()
                .next()
                .unwrap_or("invalid arguments")
                .trim()
                .to_string();
            fail(
                CliError::validation(msg)
                    .with_hint("run `erpai --help` or `erpai <group> <command> --help`"),
            );
        }
    };
    let format = cli.global.format;
    let rt =
        tokio::runtime::Runtime::new().unwrap_or_else(|e| fail(CliError::internal(e.to_string())));
    match rt.block_on(run(cli)) {
        Ok(rendered) => rendered.print(format),
        Err(e) => fail(e),
    }
}

async fn run(cli: Cli) -> erpai::error::Result<output::Rendered> {
    let g = cli.global.clone();
    match cli.command {
        Command::Apps(c) => cmd::apps::run(&g, c).await,
        Command::Tables(c) => cmd::tables::run(&g, c).await,
        Command::Columns(c) => cmd::columns::run(&g, c).await,
        Command::Records(c) => cmd::records::run(&g, c).await,
        Command::Sql(c) => cmd::sql::run(&g, c).await,
        Command::Workflows(c) => cmd::workflows::run(&g, c).await,
        Command::Layouts(c) => cmd::layouts::run(&g, c).await,
        Command::Forms(c) => cmd::forms::run(&g, c).await,
        Command::Documents(c) => cmd::documents::run(&g, c).await,
        Command::Login(a) => auth::login(&g, a).await,
        Command::Logout => auth::logout(&g).await,
        Command::Whoami => auth::whoami(&g).await,
        Command::Doctor => auth::doctor(&g).await,
        Command::Settings => cmd::settings::run(&g),
    }
}

fn fail(e: CliError) -> ! {
    eprintln!(
        "{}",
        serde_json::to_string(&e.to_json()).unwrap_or_default()
    );
    std::process::exit(e.code.exit_code());
}
