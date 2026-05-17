//! Axum handlers for dashcam-archive.

use std::{
    convert::Infallible,
    sync::{Arc, OnceLock},
};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse, sse::Event},
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokimo_bus_auth::TokimoUser;
use tokimo_bus_client::BusClient;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::{
    core::{encoder, ffmpeg::FfmpegPaths, pipeline},
    db::{
        entities::{merge_groups, merge_runs, sources, warnings},
        repos::{
            merge_runs_repo::MergeRunsRepo,
            sources_repo::{SourceInput, SourcesRepo},
            warnings_repo::WarningsRepo,
        },
    },
    orchestrator::Orchestrator,
};

pub struct AppCtx {
    pub db: DatabaseConnection,
    pub client: Arc<OnceLock<Arc<BusClient>>>,
    pub ffmpeg_paths: Arc<tokio::sync::RwLock<FfmpegPaths>>,
    pub orchestrator: Orchestrator,
}

pub struct AppError {
    pub status: StatusCode,
    pub message: String,
}
impl AppError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }
    fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
    fn unavailable(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: msg.into(),
        }
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "error": self.message }))).into_response()
    }
}
impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        Self::internal(error.to_string())
    }
}
impl From<sea_orm::DbErr> for AppError {
    fn from(error: sea_orm::DbErr) -> Self {
        Self::internal(format!("db: {error}"))
    }
}

#[derive(Serialize)]
pub struct HealthResp {
    status: &'static str,
    app: &'static str,
    bus_bound: bool,
    ffmpeg_available: bool,
}
pub async fn health(State(ctx): State<Arc<AppCtx>>) -> Json<HealthResp> {
    Json(HealthResp {
        status: "ok",
        app: "dashcam-archive",
        bus_bound: ctx.client.get().is_some(),
        ffmpeg_available: ctx.ffmpeg_paths.read().await.ffmpeg.is_some(),
    })
}

pub async fn encoders(State(ctx): State<Arc<AppCtx>>) -> Json<Vec<encoder::EncoderInfo>> {
    Json(encoder::registry(&ctx.ffmpeg_paths.read().await.clone()))
}

#[derive(Debug, Deserialize)]
pub struct SourceReq {
    pub name: Option<String>,
    pub src_source_id: Uuid,
    pub src_source_type: String,
    pub src_path: Option<String>,
    pub dst_source_id: Uuid,
    pub dst_source_type: String,
    pub dst_path: Option<String>,
    pub input_path: Option<String>,
    pub output_path: Option<String>,
    pub encoder: Option<String>,
    pub encoder_params: Option<Value>,
    pub max_gap_seconds: Option<i32>,
    pub max_group_duration_seconds: Option<i32>,
    pub monthly_subdirs: Option<String>,
    pub allow_combined_input: Option<bool>,
    pub no_broken_split: Option<bool>,
    pub trigger_mode: Option<String>,
    pub cron_expr: Option<String>,
    pub cron: Option<String>,
    pub watcher_debounce_secs: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SourcePatchReq {
    pub name: Option<String>,
    pub src_source_id: Option<Uuid>,
    pub src_source_type: Option<String>,
    pub src_path: Option<String>,
    pub dst_source_id: Option<Uuid>,
    pub dst_source_type: Option<String>,
    pub dst_path: Option<String>,
    pub input_path: Option<String>,
    pub output_path: Option<String>,
    pub encoder: Option<String>,
    pub encoder_params: Option<Value>,
    pub max_gap_seconds: Option<i32>,
    pub max_group_duration_seconds: Option<i32>,
    pub monthly_subdirs: Option<String>,
    pub allow_combined_input: Option<bool>,
    pub no_broken_split: Option<bool>,
    pub trigger_mode: Option<String>,
    pub cron_expr: Option<String>,
    pub cron: Option<String>,
    pub watcher_debounce_secs: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SourceDto {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub src_source_id: Uuid,
    pub src_source_type: String,
    pub src_path: String,
    pub dst_source_id: Uuid,
    pub dst_source_type: String,
    pub dst_path: String,
    pub encoder: String,
    pub encoder_params: Value,
    pub max_gap_seconds: i32,
    pub max_group_duration_seconds: i32,
    pub monthly_subdirs: String,
    pub allow_combined_input: bool,
    pub no_broken_split: bool,
    pub trigger_mode: String,
    pub cron_expr: Option<String>,
    pub watcher_debounce_secs: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl From<sources::Model> for SourceDto {
    fn from(model: sources::Model) -> Self {
        Self {
            id: model.id,
            user_id: model.user_id,
            name: model.name,
            src_source_id: model.src_source_id,
            src_source_type: model.src_source_type,
            src_path: model.src_path,
            dst_source_id: model.dst_source_id,
            dst_source_type: model.dst_source_type,
            dst_path: model.dst_path,
            encoder: model.encoder,
            encoder_params: model.encoder_params,
            max_gap_seconds: model.max_gap_seconds,
            max_group_duration_seconds: model.max_group_duration_seconds,
            monthly_subdirs: model.monthly_subdirs,
            allow_combined_input: model.allow_combined_input,
            no_broken_split: model.no_broken_split,
            trigger_mode: model.trigger_mode,
            cron_expr: model.cron_expr,
            watcher_debounce_secs: model.watcher_debounce_secs,
            enabled: model.enabled,
            created_at: model.created_at.with_timezone(&Utc),
            updated_at: model.updated_at.with_timezone(&Utc),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RunDto {
    pub id: Uuid,
    pub source_id: Uuid,
    pub trigger: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub total_groups: i32,
    pub ok_groups: i32,
    pub downgraded_groups: i32,
    pub failed_groups: i32,
    pub bytes_in: Option<i64>,
    pub bytes_out: Option<i64>,
    pub folder_breaker_tripped: bool,
    pub log_summary: Option<String>,
}
impl From<merge_runs::Model> for RunDto {
    fn from(model: merge_runs::Model) -> Self {
        Self {
            id: model.id,
            source_id: model.source_id,
            trigger: model.trigger,
            status: model.status,
            started_at: model.started_at.with_timezone(&Utc),
            finished_at: model.finished_at.map(|v| v.with_timezone(&Utc)),
            total_groups: model.total_groups,
            ok_groups: model.ok_groups,
            downgraded_groups: model.downgraded_groups,
            failed_groups: model.failed_groups,
            bytes_in: model.bytes_in,
            bytes_out: model.bytes_out,
            folder_breaker_tripped: model.folder_breaker_tripped,
            log_summary: model.log_summary,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct GroupDto {
    pub id: Uuid,
    pub camera_key: String,
    pub start_dt: Option<DateTime<Utc>>,
    pub end_dt: Option<DateTime<Utc>>,
    pub output_path: String,
    pub decision: String,
    pub status: String,
    pub warning_level: String,
    pub duration_secs: Option<f64>,
    pub bytes_in: Option<i64>,
    pub bytes_out: Option<i64>,
    pub abort_reason: Option<String>,
}
impl From<merge_groups::Model> for GroupDto {
    fn from(model: merge_groups::Model) -> Self {
        Self {
            id: model.id,
            camera_key: model.camera_key,
            start_dt: model.start_dt.map(|v| v.with_timezone(&Utc)),
            end_dt: model.end_dt.map(|v| v.with_timezone(&Utc)),
            output_path: model.output_path,
            decision: model.decision,
            status: model.status,
            warning_level: model.warning_level,
            duration_secs: model.duration_secs,
            bytes_in: model.bytes_in,
            bytes_out: model.bytes_out,
            abort_reason: model.abort_reason,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct WarningDto {
    pub id: Uuid,
    pub group_id: Uuid,
    pub warning_key: String,
    pub count: i32,
    pub first_example: Option<String>,
}
impl From<warnings::Model> for WarningDto {
    fn from(model: warnings::Model) -> Self {
        Self {
            id: model.id,
            group_id: model.group_id,
            warning_key: model.warning_key,
            count: model.count,
            first_example: model.first_example,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct RunDetailDto {
    pub run: RunDto,
    pub groups: Vec<GroupDto>,
    pub warnings: Vec<WarningDto>,
}

pub async fn list_sources(State(ctx): State<Arc<AppCtx>>, user: TokimoUser) -> Result<Json<Vec<SourceDto>>, AppError> {
    let user_id = parse_user_id(&user.user_id)?;
    Ok(Json(
        SourcesRepo::list(&ctx.db, user_id)
            .await?
            .into_iter()
            .map(SourceDto::from)
            .collect(),
    ))
}
pub async fn create_source(
    State(ctx): State<Arc<AppCtx>>,
    user: TokimoUser,
    Json(req): Json<SourceReq>,
) -> Result<Json<SourceDto>, AppError> {
    let dto = SourceDto::from(
        SourcesRepo::create(
            &ctx.db,
            source_input(&ctx.db, req, parse_user_id(&user.user_id)?).await?,
        )
        .await?,
    );
    let orchestrator = ctx.orchestrator.clone();
    tokio::spawn(async move { orchestrator.reload_supervisors().await });
    Ok(Json(dto))
}
pub async fn get_source(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
) -> Result<Json<SourceDto>, AppError> {
    let source = SourcesRepo::get(&ctx.db, id, parse_user_id(&user.user_id)?)
        .await?
        .ok_or_else(|| AppError::not_found("source not found"))?;
    Ok(Json(SourceDto::from(source)))
}
pub async fn update_source(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
    Json(req): Json<SourcePatchReq>,
) -> Result<Json<SourceDto>, AppError> {
    let user_id = parse_user_id(&user.user_id)?;
    let existing = SourcesRepo::get(&ctx.db, id, user_id)
        .await?
        .ok_or_else(|| AppError::not_found("source not found"))?;
    let source = SourcesRepo::update(
        &ctx.db,
        id,
        user_id,
        patch_source_input(&ctx.db, req, existing, user_id).await?,
    )
    .await?
    .ok_or_else(|| AppError::not_found("source not found"))?;
    let orchestrator = ctx.orchestrator.clone();
    tokio::spawn(async move { orchestrator.reload_supervisors().await });
    Ok(Json(SourceDto::from(source)))
}
#[derive(Serialize)]
pub struct DeleteResp {
    deleted: u64,
}
pub async fn delete_source(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
) -> Result<Json<DeleteResp>, AppError> {
    let deleted = SourcesRepo::delete(&ctx.db, id, parse_user_id(&user.user_id)?).await?;
    let orchestrator = ctx.orchestrator.clone();
    tokio::spawn(async move { orchestrator.reload_supervisors().await });
    Ok(Json(DeleteResp { deleted }))
}
#[derive(Serialize)]
pub struct RunCreatedResp {
    run_id: Uuid,
}
pub async fn run_source(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
) -> Result<Json<RunCreatedResp>, AppError> {
    if !ctx.ffmpeg_paths.read().await.is_available() {
        return Err(AppError::unavailable("ffmpeg unavailable"));
    }
    let user_id = parse_user_id(&user.user_id)?;
    Ok(Json(RunCreatedResp {
        run_id: ctx.orchestrator.enqueue_run(id, user_id).await?,
    }))
}
pub async fn dry_run_source(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
) -> Result<Json<pipeline::DryRunPlan>, AppError> {
    let user_id = parse_user_id(&user.user_id)?;
    let source = SourcesRepo::get(&ctx.db, id, user_id)
        .await?
        .ok_or_else(|| AppError::not_found("source not found"))?;
    let plan = pipeline::dry_run_plan(&ctx.db, source).await?;
    Ok(Json(plan))
}
pub async fn list_source_runs(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
) -> Result<Json<Vec<RunDto>>, AppError> {
    let user_id = parse_user_id(&user.user_id)?;
    SourcesRepo::get(&ctx.db, id, user_id)
        .await?
        .ok_or_else(|| AppError::not_found("source not found"))?;
    Ok(Json(
        MergeRunsRepo::list_for_source(&ctx.db, id, user_id)
            .await?
            .into_iter()
            .map(RunDto::from)
            .collect(),
    ))
}
pub async fn get_run(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
) -> Result<Json<RunDetailDto>, AppError> {
    let run = MergeRunsRepo::get_run_for_user(&ctx.db, id, parse_user_id(&user.user_id)?)
        .await?
        .ok_or_else(|| AppError::not_found("run not found"))?;
    let group_models = MergeRunsRepo::list_groups(&ctx.db, id).await?;
    let group_ids = group_models.iter().map(|group| group.id).collect();
    let groups = group_models.into_iter().map(GroupDto::from).collect();
    let warnings = WarningsRepo::list_for_groups(&ctx.db, group_ids)
        .await?
        .into_iter()
        .map(WarningDto::from)
        .collect();
    Ok(Json(RunDetailDto {
        run: RunDto::from(run),
        groups,
        warnings,
    }))
}
#[derive(Serialize)]
pub struct CancelResp {
    cancelling: bool,
}
pub async fn cancel_run(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
) -> Result<Json<CancelResp>, AppError> {
    MergeRunsRepo::get_run_for_user(&ctx.db, id, parse_user_id(&user.user_id)?)
        .await?
        .ok_or_else(|| AppError::not_found("run not found"))?;
    Ok(Json(CancelResp {
        cancelling: ctx.orchestrator.cancel_run(id).await?,
    }))
}
pub async fn stream_run(
    State(ctx): State<Arc<AppCtx>>,
    Path(id): Path<Uuid>,
    user: TokimoUser,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, AppError> {
    let user_id = parse_user_id(&user.user_id)?;
    MergeRunsRepo::get_run_for_user(&ctx.db, id, user_id)
        .await?
        .ok_or_else(|| AppError::not_found("run not found"))?;
    let stream = BroadcastStream::new(ctx.orchestrator.subscribe_progress()).filter_map(move |event| async move {
        match event {
            Ok(progress) if progress.run_id == id => serde_json::to_string(&progress)
                .ok()
                .map(|json| Ok(Event::default().event("progress").data(json))),
            _ => None,
        }
    });
    Ok(Sse::new(stream))
}

fn parse_user_id(user_id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(user_id).map_err(|error| AppError::bad_request(format!("invalid user id: {error}")))
}
async fn source_input(db: &DatabaseConnection, req: SourceReq, user_id: Uuid) -> Result<SourceInput, AppError> {
    let src_source_type = req.src_source_type.trim().to_string();
    let dst_source_type = req.dst_source_type.trim().to_string();
    validate_vfs_source(db, req.src_source_id, &src_source_type, "src_source_id").await?;
    validate_vfs_source(db, req.dst_source_id, &dst_source_type, "dst_source_id").await?;

    let raw_src = req
        .src_path
        .or(req.input_path)
        .ok_or_else(|| AppError::bad_request("src_path (or input_path) is required"))?;
    let src_path = normalize_vfs_path(&raw_src, "src_path")?;
    let raw_dst = req
        .dst_path
        .or(req.output_path)
        .ok_or_else(|| AppError::bad_request("dst_path (or output_path) is required"))?;
    let dst_path = normalize_vfs_path(&raw_dst, "dst_path")?;
    let allow_combined = req.allow_combined_input.unwrap_or(false);
    validate_source_paths(req.name.as_deref(), &src_path, &dst_path, allow_combined)?;
    let name = source_name(req.name, &src_path, &dst_path)?;

    Ok(SourceInput {
        user_id,
        name,
        src_source_id: req.src_source_id,
        src_source_type,
        dst_source_id: req.dst_source_id,
        dst_source_type,
        src_path,
        dst_path,
        encoder: req.encoder.unwrap_or_else(|| "auto".to_string()),
        encoder_params: req.encoder_params.unwrap_or_else(|| serde_json::json!({})),
        max_gap_seconds: req.max_gap_seconds.unwrap_or(60),
        max_group_duration_seconds: req.max_group_duration_seconds.unwrap_or(0),
        monthly_subdirs: req.monthly_subdirs.unwrap_or_else(|| "auto".to_string()),
        allow_combined_input: allow_combined,
        no_broken_split: req.no_broken_split.unwrap_or(false),
        trigger_mode: req.trigger_mode.unwrap_or_else(|| "manual_only".to_string()),
        cron_expr: req.cron_expr.or(req.cron),
        watcher_debounce_secs: req.watcher_debounce_secs.unwrap_or(60),
        enabled: req.enabled.unwrap_or(true),
    })
}

async fn patch_source_input(
    db: &DatabaseConnection,
    req: SourcePatchReq,
    existing: sources::Model,
    user_id: Uuid,
) -> Result<SourceInput, AppError> {
    let src_source_id = req.src_source_id.unwrap_or(existing.src_source_id);
    let src_source_type = req
        .src_source_type
        .unwrap_or(existing.src_source_type)
        .trim()
        .to_string();
    let dst_source_id = req.dst_source_id.unwrap_or(existing.dst_source_id);
    let dst_source_type = req
        .dst_source_type
        .unwrap_or(existing.dst_source_type)
        .trim()
        .to_string();
    validate_vfs_source(db, src_source_id, &src_source_type, "src_source_id").await?;
    validate_vfs_source(db, dst_source_id, &dst_source_type, "dst_source_id").await?;

    let src_path = req
        .src_path
        .or(req.input_path)
        .map(|path| normalize_vfs_path(&path, "src_path"))
        .transpose()?
        .unwrap_or(existing.src_path);
    let dst_path = req
        .dst_path
        .or(req.output_path)
        .map(|path| normalize_vfs_path(&path, "dst_path"))
        .transpose()?
        .unwrap_or(existing.dst_path);
    let allow_combined_input = req.allow_combined_input.unwrap_or(existing.allow_combined_input);
    validate_source_paths(
        req.name.as_deref().or(Some(existing.name.as_str())),
        &src_path,
        &dst_path,
        allow_combined_input,
    )?;
    let name = req.name.unwrap_or(existing.name);
    let name = source_name(Some(name), &src_path, &dst_path)?;

    Ok(SourceInput {
        user_id,
        name,
        src_source_id,
        src_source_type,
        dst_source_id,
        dst_source_type,
        src_path,
        dst_path,
        encoder: req.encoder.unwrap_or(existing.encoder),
        encoder_params: req.encoder_params.unwrap_or(existing.encoder_params),
        max_gap_seconds: req.max_gap_seconds.unwrap_or(existing.max_gap_seconds),
        max_group_duration_seconds: req
            .max_group_duration_seconds
            .unwrap_or(existing.max_group_duration_seconds),
        monthly_subdirs: req.monthly_subdirs.unwrap_or(existing.monthly_subdirs),
        allow_combined_input,
        no_broken_split: req.no_broken_split.unwrap_or(existing.no_broken_split),
        trigger_mode: req.trigger_mode.unwrap_or(existing.trigger_mode),
        cron_expr: req.cron_expr.or(req.cron).or(existing.cron_expr),
        watcher_debounce_secs: req.watcher_debounce_secs.unwrap_or(existing.watcher_debounce_secs),
        enabled: req.enabled.unwrap_or(existing.enabled),
    })
}

async fn validate_vfs_source(
    db: &DatabaseConnection,
    source_id: Uuid,
    source_type: &str,
    field: &str,
) -> Result<(), AppError> {
    if source_type.trim().is_empty() {
        return Err(AppError::bad_request(format!("{field} type is empty")));
    }
    let stmt = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT type FROM public.vfs WHERE id = $1",
        [source_id.into()],
    );
    let row = db
        .query_one_raw(stmt)
        .await
        .map_err(|error| AppError::internal(format!("vfs lookup failed for {field}: {error}")))?;
    let Some(row) = row else {
        return Err(AppError::bad_request(format!(
            "{field} does not reference an existing VFS source"
        )));
    };
    let actual: String = row
        .try_get("", "type")
        .map_err(|error| AppError::internal(format!("vfs lookup failed for {field}: {error}")))?;
    if actual != source_type {
        return Err(AppError::bad_request(format!(
            "{field} type mismatch: expected {source_type}, got {actual}"
        )));
    }
    Ok(())
}

fn normalize_vfs_path(raw: &str, field: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request(format!("{field} is empty")));
    }
    if trimmed.contains("vfs://") {
        return Err(AppError::bad_request(format!("{field} must not contain 'vfs://'")));
    }
    if trimmed.starts_with('/') {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("/{trimmed}"))
    }
}

fn source_name(name: Option<String>, src_path: &str, dst_path: &str) -> Result<String, AppError> {
    let final_name = name.unwrap_or_else(|| format!("{src_path} -> {dst_path}"));
    if final_name.trim().is_empty() {
        return Err(AppError::bad_request("name is empty"));
    }
    Ok(final_name)
}

fn validate_source_paths(
    name: Option<&str>,
    src_path: &str,
    dst_path: &str,
    allow_combined_input: bool,
) -> Result<(), AppError> {
    if name.is_some_and(|value| value.trim().is_empty()) {
        return Err(AppError::bad_request("name is empty"));
    }
    if src_path.trim().is_empty() {
        return Err(AppError::bad_request("src_path is empty"));
    }
    if dst_path.trim().is_empty() {
        return Err(AppError::bad_request("dst_path is empty"));
    }
    if !src_path.starts_with('/') {
        return Err(AppError::bad_request("src_path must be an absolute VFS path"));
    }
    if !dst_path.starts_with('/') {
        return Err(AppError::bad_request("dst_path must be an absolute VFS path"));
    }
    if !allow_combined_input && src_path.contains("_combined") {
        return Err(AppError::bad_request(
            "src_path contains '_combined' but allow_combined_input is false",
        ));
    }
    Ok(())
}
