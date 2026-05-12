//! Axum handlers for dashcam-archive.

use axum::Json;

pub struct AppCtx;

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "app": "dashcam-archive",
    }))
}
