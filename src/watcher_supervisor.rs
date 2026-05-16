//! Watcher supervisor — triggers merge runs when source files change.
//!
//! Two backends:
//! - **Local** (`src_source_type == "local"`): `notify` crate, recursive FS watch + own debounce.
//! - **Remote** (smb/sftp/s3/ftp/…): periodic VFS `list()` poll every 60 s, comparing snapshots.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::Duration,
};

use chrono::DateTime;
use notify::{
    EventKind, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    core::{naming::is_video_file, vfs_source::build_vfs},
    cron_supervisor::ActiveRuns,
    db::repos::sources_repo::SourcesRepo,
    orchestrator::Orchestrator,
};

/// Fixed poll interval for remote VFS sources.
const REMOTE_POLL_SECS: u64 = 60;

/// Minimum local debounce window (seconds).
const MIN_DEBOUNCE_SECS: u64 = 5;

struct WatcherHandle {
    /// Dropping this sender signals the spawned task to stop.
    _shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

pub struct WatcherSupervisor {
    db: DatabaseConnection,
    orchestrator: Orchestrator,
    active_runs: ActiveRuns,
    handles: Arc<Mutex<HashMap<Uuid, WatcherHandle>>>,
}

impl WatcherSupervisor {
    pub async fn new(
        db: DatabaseConnection,
        orchestrator: Orchestrator,
        active_runs: ActiveRuns,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            db,
            orchestrator,
            active_runs,
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
            .filter(|s| s.trigger_mode.to_lowercase().contains("watcher"))
            .collect();

        for source in watcher_sources {
            // Guard against negative/zero watcher_debounce_secs before casting to u64.
            let debounce_secs = source.watcher_debounce_secs.max(0) as u64;
            let debounce = Duration::from_secs(debounce_secs.max(MIN_DEBOUNCE_SECS));
            let source_id = source.id;
            let user_id = source.user_id;

            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

            // Backend is determined by src_source_type, not by whether resolve_real_path succeeds.
            if source.src_source_type.to_lowercase() == "local" {
                self.spawn_local_watcher(source_id, user_id, source.src_path.clone(), debounce, shutdown_rx);
            } else {
                let vfs = match build_vfs(&self.db, source.src_source_id, &source.src_source_type).await {
                    Ok(v) => v,
                    Err(error) => {
                        tracing::warn!(%source_id, %error, "watcher: failed to build VFS, skipping source");
                        continue;
                    }
                };
                self.spawn_remote_poller(source_id, user_id, vfs, source.src_path.clone(), debounce, shutdown_rx);
            }

            handles.insert(
                source_id,
                WatcherHandle {
                    _shutdown_tx: shutdown_tx,
                },
            );
        }

        tracing::info!(
            count = handles.len(),
            "dashcam-archive: WatcherSupervisor reload complete"
        );
        Ok(())
    }

    #[allow(dead_code)]
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
        let orchestrator = self.orchestrator.clone();
        let active_runs = Arc::clone(&self.active_runs);

        tokio::spawn(async move {
            // Bridge the sync notify callback into the async world.
            let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<()>(16);

            let mut watcher = match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    let is_video_create_or_move = matches!(
                        event.kind,
                        EventKind::Create(_)
                            | EventKind::Modify(ModifyKind::Name(RenameMode::To | RenameMode::Both | RenameMode::Any,))
                    ) && event.paths.iter().any(|p| is_video_file(p));

                    if is_video_create_or_move {
                        let _ = event_tx.blocking_send(());
                    }
                }
            }) {
                Ok(w) => w,
                Err(error) => {
                    tracing::error!(%source_id, %error, "watcher: notify init failed");
                    return;
                }
            };

            if let Err(error) = watcher.watch(Path::new(&local_path), RecursiveMode::Recursive) {
                tracing::error!(%source_id, %error, path = %local_path, "watcher: watch() failed");
                return;
            }

            tracing::info!(%source_id, path = %local_path, ?debounce, "watcher: local notify started");

            // Debounce loop: each video event resets the deadline; once it expires with no
            // further events the trigger fires. `biased` ensures incoming events (branch 2)
            // are always drained before the timer (branch 3), so rapid file copies keep
            // resetting the deadline correctly.
            let mut deadline: Option<tokio::time::Instant> = None;

            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown_rx => break,
                    result = event_rx.recv() => match result {
                        Some(()) => deadline = Some(tokio::time::Instant::now() + debounce),
                        None => break,
                    },
                    _ = async {
                        match deadline {
                            Some(d) => tokio::time::sleep_until(d).await,
                            None => std::future::pending::<()>().await,
                        }
                    } => {
                        deadline = None;
                        maybe_trigger(&orchestrator, &active_runs, source_id, user_id).await;
                    }
                }
            }

            // Keep watcher alive until the loop exits so the OS watch stays registered.
            drop(watcher);
            tracing::info!(%source_id, "watcher: local notify stopped");
        });
    }

    fn spawn_remote_poller(
        &self,
        source_id: Uuid,
        user_id: Uuid,
        vfs: Arc<tokimo_vfs::Vfs>,
        src_path: String,
        debounce: Duration,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let orchestrator = self.orchestrator.clone();
        let active_runs = Arc::clone(&self.active_runs);

        tokio::spawn(async move {
            // (vfs_path → (size, mtime)) — populated on first scan; new paths compared afterwards.
            type FileKey = (u64, Option<DateTime<chrono::Utc>>);
            let mut last_seen: HashMap<String, FileKey> = HashMap::new();
            let mut first_tick = true;
            // Debounce deadline — fires independently of the 60 s poll interval so a small
            // debounce does not have to wait a full extra poll cycle.
            let mut debounce_deadline: Option<tokio::time::Instant> = None;

            let poll = Duration::from_secs(REMOTE_POLL_SECS);
            let mut ticker = tokio::time::interval(poll);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            tracing::info!(%source_id, path = %src_path, poll_secs = REMOTE_POLL_SECS, ?debounce, "watcher: remote poll started");

            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown_rx => break,
                    // Debounce timer: fires as soon as debounce has elapsed after the last
                    // new-file detection, regardless of where the next poll tick falls.
                    _ = async {
                        match debounce_deadline {
                            Some(d) => tokio::time::sleep_until(d).await,
                            None => std::future::pending::<()>().await,
                        }
                    } => {
                        debounce_deadline = None;
                        maybe_trigger(&orchestrator, &active_runs, source_id, user_id).await;
                    }
                    _ = ticker.tick() => {
                        let files = match vfs.list(Path::new(&src_path)).await {
                            Ok(f) => f,
                            Err(error) => {
                                tracing::warn!(%source_id, %error, "watcher: VFS list failed, will retry");
                                continue;
                            }
                        };

                        let mut current: HashMap<String, FileKey> = HashMap::new();
                        let mut new_files = false;

                        for f in files {
                            if f.is_dir || !is_video_file(Path::new(&f.name)) {
                                continue;
                            }
                            let key: FileKey = (f.size, f.modified);
                            // Only flag brand-new paths — ignore size/mtime changes on existing files.
                            if !first_tick && !last_seen.contains_key(&f.path) {
                                new_files = true;
                            }
                            current.insert(f.path, key);
                        }

                        last_seen = current;

                        if !first_tick && new_files {
                            tracing::debug!(%source_id, "watcher: remote new files detected, debounce timer reset");
                            debounce_deadline = Some(tokio::time::Instant::now() + debounce);
                        }

                        first_tick = false;
                    }
                }
            }

            tracing::info!(%source_id, "watcher: remote poll stopped");
        });
    }
}

/// Trigger a watcher-driven merge run unless this source already has an active run.
/// Uses the shared `ActiveRuns` guard — same dedup mechanism as `CronSupervisor`.
async fn maybe_trigger(
    orchestrator: &Orchestrator,
    active_runs: &Arc<Mutex<HashSet<Uuid>>>,
    source_id: Uuid,
    user_id: Uuid,
) {
    {
        let mut active = active_runs.lock().await;
        if !active.insert(source_id) {
            tracing::info!(%source_id, "watcher: skipped — run already active");
            return;
        }
    }

    let orchestrator = orchestrator.clone();
    let active_runs = Arc::clone(active_runs);
    tokio::spawn(async move {
        if let Err(error) = orchestrator
            .run_source_with_trigger(source_id, user_id, "watcher")
            .await
        {
            tracing::warn!(%source_id, %error, "watcher: triggered run failed");
        } else {
            tracing::info!(%source_id, "watcher: triggered run completed");
        }
        active_runs.lock().await.remove(&source_id);
    });
}
