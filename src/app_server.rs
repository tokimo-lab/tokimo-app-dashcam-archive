//! Axum HTTP server for the dashcam-archive app.

use std::sync::Arc;

use axum::{
    Router,
    routing::get,
};
use tokimo_bus_protocol::{BusListener, DataPlaneSocket};
use tracing::{error, info};

use crate::{handlers, handlers::AppCtx};

pub async fn spawn(service: &str, ctx: Arc<AppCtx>) -> anyhow::Result<DataPlaneSocket> {
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
    Router::new().route("/health", get(handlers::health)).with_state(ctx)
}
