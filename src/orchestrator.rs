use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use sea_orm::DatabaseConnection;
use tokimo_bus_client::BusClient;
use tokimo_bus_protocol::CallerCtx;
use tokio::sync::{Mutex, Semaphore, broadcast};
use uuid::Uuid;

use crate::{
    core::{
        ffmpeg::{CancellationToken, FfmpegPaths},
        pipeline::{Pipeline, ProgressEvent},
    },
    db::repos::{merge_runs_repo::MergeRunsRepo, sources_repo::SourcesRepo},
};

// Forward declaration for cron/watcher supervisors.
// Actual supervisor instances are owned and managed by main.rs, not Orchestrator.
// Orchestrator provides lifecycle hooks (start_supervisors, reload_supervisors) that
// main.rs will wire to the supervisor instances.

type ReloadHook =
    Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct Orchestrator {
    db: DatabaseConnection,
    ffmpeg_paths: Arc<tokio::sync::RwLock<FfmpegPaths>>,
    semaphore: Arc<Semaphore>,
    active_runs: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    progress: broadcast::Sender<ProgressEvent>,
    bus: Arc<OnceLock<Arc<BusClient>>>,
    /// Optional callback to reload supervisors (wired by main.rs).
    reload_hook: Arc<OnceLock<ReloadHook>>,
}

impl Orchestrator {
    pub fn new(
        db: DatabaseConnection,
        ffmpeg_paths: Arc<tokio::sync::RwLock<FfmpegPaths>>,
        workers: usize,
        bus: Arc<OnceLock<Arc<BusClient>>>,
    ) -> Self {
        let (progress, _) = broadcast::channel(256);
        Self {
            db,
            ffmpeg_paths,
            semaphore: Arc::new(Semaphore::new(workers.max(1))),
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            progress,
            bus,
            reload_hook: Arc::new(OnceLock::new()),
        }
    }

    /// Wire a reload callback for supervisors (called by main.rs during setup).
    pub fn set_reload_hook<F>(&self, hook: F)
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let _ = self.reload_hook.set(Box::new(hook));
    }

    pub async fn enqueue_run(&self, source_id: Uuid, user_id: Uuid) -> anyhow::Result<Uuid> {
        self.enqueue_run_with_trigger(source_id, user_id, "manual").await
    }

    pub async fn enqueue_run_with_trigger(
        &self,
        source_id: Uuid,
        user_id: Uuid,
        trigger: &str,
    ) -> anyhow::Result<Uuid> {
        let source = SourcesRepo::get(&self.db, source_id, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("source not found"))?;
        let run = MergeRunsRepo::create_run(&self.db, source.id, trigger.to_string()).await?;
        let run_id = run.id;
        let token = CancellationToken::default();
        self.active_runs.lock().await.insert(run_id, token.clone());
        let this = self.clone();
        tokio::spawn(async move {
            this.execute_run(run, source, token, user_id).await;
        });
        Ok(run_id)
    }

    pub async fn run_source_with_trigger(&self, source_id: Uuid, user_id: Uuid, trigger: &str) -> anyhow::Result<Uuid> {
        let source = SourcesRepo::get(&self.db, source_id, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("source not found"))?;
        let run = MergeRunsRepo::create_run(&self.db, source.id, trigger.to_string()).await?;
        let run_id = run.id;
        let token = CancellationToken::default();
        self.active_runs.lock().await.insert(run_id, token.clone());
        self.execute_run(run, source, token, user_id).await;
        Ok(run_id)
    }

    async fn execute_run(
        &self,
        run: crate::db::entities::merge_runs::Model,
        source: crate::db::entities::sources::Model,
        token: CancellationToken,
        user_id: Uuid,
    ) {
        let permit = match self.semaphore.acquire().await {
            Ok(permit) => permit,
            Err(error) => {
                tracing::warn!(%error, "dashcam-archive: worker semaphore closed");
                return;
            }
        };
        let pipeline = Pipeline::new(
            self.db.clone(),
            Arc::clone(&self.ffmpeg_paths),
            self.progress.clone(),
            Arc::clone(&self.bus),
            user_id,
        );
        let result = pipeline.run(run.clone(), source, token).await;
        if let Err(error) = result {
            let status = if error.to_string().contains("cancelled") {
                "cancelled"
            } else {
                "failed"
            };
            let error_text = error.to_string();
            tracing::error!(run_id=%run.id, %error, "dashcam-archive: run failed");
            let _ = MergeRunsRepo::set_status_with_summary(&self.db, run.id, status, Some(error_text.clone())).await;
            if let Some(client) = self.bus.get() {
                let caller = CallerCtx {
                    user_id: Some(user_id.to_string()),
                    request_id: uuid::Uuid::new_v4().to_string(),
                    workspace: None,
                };
                let error_msg = if status == "failed" {
                    Some(error_text.clone())
                } else {
                    None
                };
                match serde_json::to_vec(&serde_json::json!({
                    "job_id": run.id,
                    "status": status,
                    "error": error_msg,
                })) {
                    Ok(bytes) => {
                        if let Err(error) = client.invoke("task_queue", "complete_job", bytes, caller).await {
                            tracing::warn!(%error, "dashcam-archive: task_queue complete_job bus call failed");
                        }
                    }
                    Err(error) => tracing::warn!(
                        %error,
                        "dashcam-archive: task_queue complete_job payload serialize failed"
                    ),
                }
            }
            let _ = self.progress.send(ProgressEvent {
                run_id: run.id,
                phase: status.to_string(),
                group_count: 0,
                ok_count: 0,
                failed_count: usize::from(status == "failed"),
                current_file: Some(error_text),
                percent: 100.0,
            });
        }
        self.active_runs.lock().await.remove(&run.id);
        drop(permit);
    }

    pub async fn cancel_run(&self, run_id: Uuid) -> anyhow::Result<bool> {
        if let Some(token) = self.active_runs.lock().await.get(&run_id) {
            token.cancel();
            let _ = MergeRunsRepo::set_status(&self.db, run_id, "cancelled").await;
            return Ok(true);
        }
        Ok(false)
    }

    #[allow(dead_code)]
    pub async fn start_supervisors(&self) -> anyhow::Result<()> {
        // Supervisors are now managed externally (main.rs).
        // This hook is kept for backward compatibility; the actual startup
        // is done in main.rs after orchestrator construction.
        let _sources = SourcesRepo::list_enabled(&self.db).await?;
        Ok(())
    }

    pub async fn reload_supervisors(&self) -> anyhow::Result<()> {
        // TODO: Hot reload via watch channel if available.
        // Currently handlers call this on source CRUD; we invoke the registered hook.
        if let Some(hook) = self.reload_hook.get() {
            hook().await?;
        }
        Ok(())
    }

    pub fn subscribe_progress(&self) -> broadcast::Receiver<ProgressEvent> {
        self.progress.subscribe()
    }
}
