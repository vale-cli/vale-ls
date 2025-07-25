use clap::Parser;
use dashmap::DashMap;
use tower_lsp::{LspService, Server};

use vale_ls::server::Backend;
use vale_ls::vale::ValeManager;

/// The official Vale Language Server.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Path to a custom Vale binary to use instead of the managed or system binary
    #[arg(long, value_name = "PATH")]
    vale_binary: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args = Args::parse();
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let vale_manager = if let Some(custom_binary) = args.vale_binary {
        ValeManager::with_custom_binary(Some(custom_binary))
    } else {
        ValeManager::new()
    };

    let (service, socket) = LspService::build(|client| Backend {
        client,
        document_map: DashMap::new(),
        param_map: DashMap::new(),
        cli: vale_manager,
    })
    .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}
