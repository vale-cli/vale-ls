use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use clap::Parser;
use dashmap::DashMap;
use tower::Service;
use tower_lsp::jsonrpc::{Request, Response};
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
        versions: Arc::new(DashMap::new()),
        config_errors: Arc::new(DashMap::new()),
    })
    .finish();

    let service = ExitOnExit {
        inner: service,
        shut_down: Arc::new(AtomicBool::new(false)),
    };
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Exits the process on `exit`: 0 after `shutdown`, 1 without one, as the
/// spec asks. tower-lsp's `serve` only returns once stdin closes, so a client
/// that sends `exit` but keeps stdin open would wait on the server forever.
struct ExitOnExit<S> {
    inner: S,
    shut_down: Arc<AtomicBool>,
}

impl<S> Service<Request> for ExitOnExit<S>
where
    S: Service<Request, Response = Option<Response>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        match req.method() {
            "shutdown" => self.shut_down.store(true, Ordering::SeqCst),
            "exit" => std::process::exit(if self.shut_down.load(Ordering::SeqCst) {
                0
            } else {
                1
            }),
            _ => {}
        }
        self.inner.call(req)
    }
}
