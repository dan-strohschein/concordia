//! `concordia-mcp`: the Concordia MCP server on stdio.

use std::process::ExitCode;

use clap::Parser;

/// Concordia MCP server (stdio). Register it in `.mcp.json` as "concordia".
#[derive(Debug, Parser)]
#[command(name = "concordia-mcp", version)]
struct Cli {
    #[command(flatten)]
    args: concordia_mcp::Args,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("concordia-mcp: cannot start the async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(concordia_mcp::run(cli.args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("concordia-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
