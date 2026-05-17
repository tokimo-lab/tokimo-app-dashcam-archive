//! Axum HTTP server for the dashcam-archive app.

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use tokimo_bus_protocol::{BusListener, DataPlaneSocket};
use tracing::{error, info};

use crate::{assets, handlers, handlers::AppCtx};

pub fn spawn(service: &str, ctx: Arc<AppCtx>) -> anyhow::Result<DataPlaneSocket> {
    let (listener, socket) = BusListener::bind_for_app(service)?;
    info!(?socket, "dashcam-archive: app server listening");

    let router = build_router(ctx);

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            error!(%error, "dashcam-archive: app server stopped");
        }
    });

    Ok(socket)
}

fn build_router(ctx: Arc<AppCtx>) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/encoders", get(handlers::encoders))
        .route("/sources", get(handlers::list_sources).post(handlers::create_source))
        .route(
            "/sources/{id}",
            get(handlers::get_source)
                .patch(handlers::update_source)
                .delete(handlers::delete_source),
        )
        .route("/sources/{id}/run", post(handlers::run_source))
        .route("/sources/{id}/dry_run", post(handlers::dry_run_source))
        .route("/sources/{id}/runs", get(handlers::list_source_runs))
        .route("/runs/{id}", get(handlers::get_run))
        .route("/runs/{id}/cancel", post(handlers::cancel_run))
        .route("/runs/{id}/stream", get(handlers::stream_run))
        .route("/assets/{*path}", get(assets::serve))
        .with_state(ctx)
}
