//! Cherenkov server binary.
//!
//! Loads a YAML config, builds a hub via [`cherenkov_server::build_hub`],
//! and runs every configured transport (WebSocket, optional SSE) plus
//! optional admin until SIGINT / SIGTERM.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context as _;
use cherenkov_server::config::{LogFormat, ServerConfig};
use cherenkov_server::run;
use clap::Parser;
use tokio::signal;
use tracing::{error, info};

mod exit_code {
    pub const RUNTIME_ERROR: u8 = 1;
    pub const CONFIG_ERROR: u8 = 3;
}

#[derive(Debug, Parser)]
#[command(name = "cherenkov-server", version, about)]
struct Args {
    /// Path to the YAML configuration file.
    #[arg(short, long, value_name = "PATH", env = "CHERENKOV_CONFIG")]
    config: PathBuf,
}

fn main() -> ExitCode {
    reset_sigpipe();
    let args = Args::parse();
    match run_cli(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("cherenkov-server: {err:#}");
            ExitCode::from(exit_code_for_error(&err))
        }
    }
}

/// Restore the default SIGPIPE disposition on Unix so that piping server
/// logs into commands like `head` produces a clean exit on broken pipe
/// rather than aborting the tokio runtime mid-write.
#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: setting a signal disposition before any threads are spawned
    // is sound; this runs as the very first thing in `main()`.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

/// Map an error to a POSIX exit code so operators can distinguish config
/// problems (3) from runtime failures (1) in supervisors / systemd.
fn exit_code_for_error(err: &anyhow::Error) -> u8 {
    if err.chain().any(|c| c.is::<figment::Error>()) {
        exit_code::CONFIG_ERROR
    } else {
        exit_code::RUNTIME_ERROR
    }
}

fn run_cli(args: Args) -> anyhow::Result<()> {
    let config = ServerConfig::load(&args.config)
        .with_context(|| format!("loading config from {}", args.config.display()))?;
    init_tracing(&config);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("cherenkov-rt")
        .build()
        .context("building tokio runtime")?;

    runtime.block_on(async move { serve(config).await })
}

async fn serve(config: ServerConfig) -> anyhow::Result<()> {
    info!(
        ws_listen = %config.transport.ws.listen,
        ws_path = %config.transport.ws.path,
        sse = config.transport.sse.is_some(),
        admin = config.admin.enabled,
        broker = ?config.broker.backend,
        "cherenkov-server starting",
    );

    let handle = run(config)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    info!(
        ws_addr = %handle.ws_addr,
        sse_addr = ?handle.sse_addr,
        admin_addr = ?handle.admin_addr,
        "cherenkov-server listening"
    );

    tokio::select! {
        res = handle.wait() => {
            if let Err(err) = res {
                error!(%err, "transport stopped with error");
                return Err(anyhow::anyhow!(err.to_string()));
            }
            info!("transports stopped");
        }
        _ = shutdown_signal() => info!("shutdown signal received"),
    }
    Ok(())
}

fn init_tracing(config: &ServerConfig) {
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_new(&config.log.level)
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let registry = tracing_subscriber::registry().with(filter);
    match config.log.format {
        LogFormat::Json => {
            registry
                .with(tracing_subscriber::fmt::layer().json().with_target(true))
                .try_init()
                .ok();
        }
        LogFormat::Pretty => {
            registry
                .with(tracing_subscriber::fmt::layer().with_target(true))
                .try_init()
                .ok();
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => {
                error!(%err, "failed to register SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
