use std::path::PathBuf;

use clap::Parser;
use dashmap::DashMap;
use tower_lsp::{LspService, Server};

use vale_ls::server::Backend;
use vale_ls::vale::ValeManager;

/// The official Vale Language Server.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Path to the Vale binary to use instead of a managed or `PATH` install.
    ///
    /// The `valeBinaryPath` client setting takes precedence over this.
    #[arg(long, value_name = "PATH")]
    vale_binary: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args = Args::parse();
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        document_map: DashMap::new(),
        param_map: DashMap::new(),
        cli: ValeManager::with_custom_exe(args.vale_binary),
    })
    .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}
