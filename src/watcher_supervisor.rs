//! Watcher supervisor — triggers merge runs when source files change.
//!
//! Two backends:
//! - **Local** (`src_source_type == "local"`): `notify-debouncer-full` on the native FS path.
//! - **Remote** (smb/sftp/s3/ftp/…): periodic VFS `list()` poll comparing (path, size, mtime).

use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    time::Duration,
};

use chrono::DateTime;
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    core::{naming::is_video_file, vfs_source::build_vfs},
    db::repos::{merge_runs_repo::MergeRunsRepo, sources_repo::SourcesRepo},
    orchestrator::Orchestrator,
};

/// Minimum debounce / poll interval to avoid hammering the system.
const MIN_DEBOUNCE_SECS: u64 = 30;

struct WatcherHandle {
    /// Dropping this sender signals the spawned task to stop.
    _shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

pub struct WatcherSupervisor {
    db: DatabaseConnection,
    orchestrator: Orchestrator,
    handles: Arc<Mutex<HashMap<Uuid, WatcherHandle>>>,
}

impl WatcherSupervisor {
    pub async fn new(db: DatabaseConnection, orchestrator: Orchestrator) -> anyhow::Result<Self> {
        Ok(Self {
            db,
            orchestrator,
            handles: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.reload().await
    }

    /// Re-read all watcher-enabled sources from DB and restart watchers.
    /// All existing watchers are stopped and rebuilt from scratch.
    pub async fn reload(&self) -> anyhow::Result<()> {
        let mut handles = self.handles.lock().await;
        handles.clear(); // dropping WatcherHandle drops _shutdown_tx → tasks break out of their loops

        let sources = SourcesRepo::list_enabled(&self.db).await?;
        let watcher_sources: Vec<_> = sources
            .into_iter()
            .filter(|s| matches!(s.trigger_mode.as_str(), "watcher" | "cron_and_watcher"))
            .collect();

        for source in watcher_sources {
            let debounce = Duration::from_secs((source.watcher_debounce_secs as u64).max(MIN_DEBOUNCE_SECS));
            let source_id = source.id;
            let user_id = source.user_id;

            let vfs = match build_vfs(&self.db, source.src_source_id, &source.src_source_type).await {
                Ok(v) => v,
                Err(error) => {
                    tracing::warn!(%source_id, %error, "watcher: failed to build VFS, skipping source");
                    continue;
                }
            };

            let native_path = vfs.resolve_real_path(Path::new(&source.src_path)).await;
            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

            if let Some(local_path) = native_path {
                self.spawn_local_watcher(source_id, user_id, local_path, debounce, shutdown_rx);
            } else {
                self.spawn_remote_poller(source_id, user_id, vfs, source.src_path.clone(), debounce, shutdown_rx);
            }

            handles.insert(source_id, WatcherHandle { _shutdown_tx: shutdown_tx });
        }

        tracing::info!(count = handles.len(), "dashcam-archive: WatcherSupervisor reload complete");
        Ok(())
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        self.handles.lock().await.clear();
        tracing::info!("dashcam-archive: WatcherSupervisor shutdown");
        Ok(())
    }

    fn spawn_local_watcher(
        &self,
        source_id: Uuid,
        user_id: Uuid,
        local_path: String,
        debounce: Duration,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let db = self.db.clone();
        let orchestrator = self.orchestrator.clone();

        tokio::spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(4);

            let mut debouncer = match new_debouncer(
                debounce,
                None,
                move |result: DebounceEventResult| {
                    if let Ok(events) = result {
                        let has_video = events.iter().any(|e| e.event.paths.iter().any(|p| is_video_file(p)));
                        if has_video {
                            let _ = tx.blocking_send(());
                        }
                    }
                },
            ) {
                Ok(d) => d,
                Err(error) => {
                    tracing::error!(%source_id, %error, "watcher: notify debouncer init failed");
                    return;
                }
            };

            if let Err(error) = debouncer.watch(Path::new(&local_path), RecursiveMode::Recursive) {
                tracing::error!(%source_id, %error, path = %local_path, "watcher: watch() failed");
                return;
            }

            tracing::info!(%source_id, path = %local_path, ?debounce, "watcher: local notify started");

            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown_rx => break,
                    result = rx.recv() => match result {
                        Some(()) => maybe_enqueue(&db, &orchestrator, source_id, user_id).await,
                        None => break,
                    },
                }
            }

            drop(debouncer);
            tracing::info!(%source_id, "watcher: local notify stopped");
        });
    }

    fn spawn_remote_poller(
        &self,
        source_id: Uuid,
        user_id: Uuid,
        vfs: Arc<tokimo_vfs::Vfs>,
        src_path: String,
        interval: Duration,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let db = self.db.clone();
        let orchestrator = self.orchestrator.clone();

        tokio::spawn(async move {
            // (vfs_path → (size, mtime)) — populated on first scan; changes compared afterwards.
            type FileKey = (u64, Option<DateTime<chrono::Utc>>);
            let mut last_seen: HashMap<String, FileKey> = HashMap::new();
            let mut first_tick = true;

            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            tracing::info!(%source_id, path = %src_path, ?interval, "watcher: remote poll started");

            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown_rx => break,
                    _ = ticker.tick() => {
                        let files = match vfs.list(Path::new(&src_path)).await {
                            Ok(f) => f,
                            Err(error) => {
                                tracing::warn!(%source_id, %error, "watcher: VFS list failed, will retry");
                                continue;
                            }
                        };

                        let mut current: HashMap<String, FileKey> = HashMap::new();
                        let mut changed = false;

                        for f in files {
                            if f.is_dir {
                                continue;
                            }
                            if !is_video_file(Path::new(&f.name)) {
                                continue;
                            }
                            let key: FileKey = (f.size, f.modified);
                            if !first_tick {
                                if last_seen.get(&f.path) != Some(&key) {
                                    changed = true;
                                }
                            }
                            current.insert(f.path, key);
                        }

                        last_seen = current;

                        if !first_tick && changed {
                            maybe_enqueue(&db, &orchestrator, source_id, user_id).await;
                        }

                        first_tick = false;
                    }
                }
            }

            tracing::info!(%source_id, "watcher: remote poll stopped");
        });
    }
}

/// Enqueue a watcher-triggered merge run unless one is already active for this source.
async fn maybe_enqueue(db: &DatabaseConnection, orchestrator: &Orchestrator, source_id: Uuid, user_id: Uuid) {
    match MergeRunsRepo::has_active_run(db, source_id).await {
        Ok(true) => {
            tracing::info!(%source_id, "watcher: skipped enqueue — run already active");
            return;
        }
        Err(error) => {
            tracing::warn!(%source_id, %error, "watcher: dedup check failed, proceeding anyway");
        }
        Ok(false) => {}
    }

    match orchestrator.enqueue_run(source_id, user_id).await {
        Ok(run_id) => tracing::info!(%source_id, %run_id, "watcher: triggered run enqueued"),
        Err(error) => tracing::warn!(%source_id, %error, "watcher: enqueue_run failed"),
    }
}
