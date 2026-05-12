//! dashcam-archive app — embedded axum + UDS Tokimo app scaffold.

mod app_server;
mod cli;
mod db;
mod handlers;

use std::sync::Arc;

use clap::Parser;
use tokimo_bus_cli::TokimoAuthArgs;
use tokimo_bus_client::{BusClient, ClientConfig};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(
    name = "tokimo-app-dashcam-archive",
    about = "录像归并 — Tokimo 子 app CLI",
    long_about = "录像归并 CLI — 行车记录仪 / 监控视频按时间分组合并与转码。",
    term_width = 100
)]
pub(crate) struct Cli {
    #[command(flatten)]
    _auth: TokimoAuthArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();

    if std::env::var_os("TOKIMO_BUS_SOCKET").is_some() {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info,tokimo_bus_client=info,tokimo_app_dashcam_archive=debug".into()),
            )
            .init();
        if let Err(error) = run_server().await {
            error!(%error, "dashcam-archive: fatal");
            std::process::exit(1);
        }
        return Ok(());
    }

    cli::print_help_and_exit();
}

async fn run_server() -> anyhow::Result<()> {
    let cfg = ClientConfig::from_env().map_err(|error| anyhow::anyhow!("ClientConfig: {error}"))?;
    info!(endpoint = ?cfg.endpoint, "dashcam-archive: connecting to broker");

    let db = db::init_pool().await?;
    db::init_schema(&db).await?;
    info!("dashcam-archive: db scaffold ready");

    let ctx = Arc::new(handlers::AppCtx);
    let app_socket = app_server::spawn("dashcam-archive", Arc::clone(&ctx))
        .await
        .map_err(|error| anyhow::anyhow!("app_server spawn: {error}"))?;

    let client = BusClient::builder(cfg)
        .service("dashcam-archive", env!("CARGO_PKG_VERSION"))
        .data_plane(app_socket)
        .build()
        .await
        .map_err(|error| anyhow::anyhow!("bus build: {error}"))?;

    info!("dashcam-archive: registered with broker");

    let shutdown = {
        let client = Arc::clone(&client);
        tokio::spawn(async move { client.run_until_shutdown().await })
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("dashcam-archive: SIGINT received");
            client.shutdown();
        }
        _ = shutdown => info!("dashcam-archive: broker sent Shutdown"),
    }

    Ok(())
}
