//! dashcam-archive app — embedded axum + UDS Tokimo app.

/// Compile-time embedded app manifest, used by the db module to read the schema name.
const MANIFEST: &str = include_str!("../tokimo-app.toml");

mod app_server;
mod assets;
mod cli;
mod core;
mod cron_supervisor;
mod db;
mod handlers;
mod orchestrator;
mod watcher_supervisor;

use std::sync::{Arc, OnceLock};

use clap::Parser;
use tokimo_bus_cli::TokimoAuthArgs;
use tokimo_bus_client::{BusClient, ClientConfig};
use tokimo_bus_protocol::CallerCtx;
use tracing::{error, info, warn};

use crate::{
    core::ffmpeg::FfmpegPaths, cron_supervisor::CronSupervisor, orchestrator::Orchestrator,
    watcher_supervisor::WatcherSupervisor,
};

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
    let client_slot = Arc::new(OnceLock::new());
    let ffmpeg_paths = Arc::new(tokio::sync::RwLock::new(FfmpegPaths::from_env()));
    let workers = std::env::var("DASHCAM_ARCHIVE_PARALLEL_SOURCES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let orchestrator = Orchestrator::new(db.clone(), Arc::clone(&ffmpeg_paths), workers, Arc::clone(&client_slot));
    let ctx = Arc::new(handlers::AppCtx {
        db: db.clone(),
        client: Arc::clone(&client_slot),
        ffmpeg_paths: Arc::clone(&ffmpeg_paths),
        orchestrator,
    });

    let app_socket = app_server::spawn("dashcam-archive", Arc::clone(&ctx))
        .map_err(|error| anyhow::anyhow!("app_server spawn: {error}"))?;

    let client = Arc::new(
        BusClient::builder(cfg)
            .service("dashcam-archive", env!("CARGO_PKG_VERSION"))
            .data_plane(app_socket)
            .build()
            .await
            .map_err(|error| anyhow::anyhow!("bus build: {error}"))?,
    );
    if client_slot.set(Arc::clone(&client)).is_err() {
        warn!("dashcam-archive: BusClient slot already initialized");
    }
    probe_ffmpeg_paths(&client, &ffmpeg_paths).await;

    // Initialize supervisors.
    let cron_supervisor = Arc::new(
        CronSupervisor::new(db.clone(), ctx.orchestrator.clone())
            .await
            .map_err(|error| anyhow::anyhow!("CronSupervisor::new: {error}"))?,
    );
    let watcher_supervisor = Arc::new(
        WatcherSupervisor::new(db.clone(), ctx.orchestrator.clone(), cron_supervisor.active_runs())
            .await
            .map_err(|error| anyhow::anyhow!("WatcherSupervisor::new: {error}"))?,
    );

    // Wire reload hook so handlers can trigger supervisor reload on source CRUD.
    {
        let cron = Arc::clone(&cron_supervisor);
        let watcher = Arc::clone(&watcher_supervisor);
        ctx.orchestrator.set_reload_hook(move || {
            let cron = Arc::clone(&cron);
            let watcher = Arc::clone(&watcher);
            Box::pin(async move {
                cron.reload().await?;
                watcher.reload().await?;
                Ok(())
            })
        });
    }

    // Start supervisors.
    cron_supervisor.start().await?;
    watcher_supervisor.start().await?;

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

async fn probe_ffmpeg_paths(client: &BusClient, paths: &tokio::sync::RwLock<FfmpegPaths>) {
    let caller = CallerCtx {
        user_id: None,
        request_id: uuid::Uuid::new_v4().to_string(),
        workspace: None,
        caller_app_id: None,
    };
    let payload = serde_json::to_vec(&serde_json::json!({}));
    let Ok(payload) = payload else {
        return;
    };
    match client.invoke("media_tools", "binary_paths", payload, caller).await {
        Ok(bytes) => match serde_json::from_slice::<FfmpegPaths>(&bytes) {
            Ok(found) => *paths.write().await = found.with_env_fallbacks(),
            Err(error) => warn!(%error, "dashcam-archive: decode media_tools paths failed"),
        },
        Err(error) => warn!(%error, "dashcam-archive: media_tools binary_paths unavailable; using env fallbacks"),
    }
}
