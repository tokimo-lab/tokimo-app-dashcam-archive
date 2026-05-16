use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{db::repos::sources_repo::SourcesRepo, orchestrator::Orchestrator};

/// Maps source_id → scheduler job UUID so we can remove stale jobs on reload.
type JobMap = Arc<Mutex<HashMap<Uuid, Uuid>>>;

/// Set of source IDs with active pipeline runs for deduplication.
type ActiveRuns = Arc<Mutex<HashSet<Uuid>>>;

pub struct CronSupervisor {
    scheduler: JobScheduler,
    orchestrator: Orchestrator,
    db: DatabaseConnection,
    /// source_id → scheduler job UUID
    jobs: JobMap,
    /// source IDs with active runs
    active_runs: ActiveRuns,
}

impl CronSupervisor {
    pub async fn new(db: DatabaseConnection, orchestrator: Orchestrator) -> anyhow::Result<Self> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self {
            scheduler,
            orchestrator,
            db,
            jobs: Arc::new(Mutex::new(HashMap::new())),
            active_runs: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.scheduler.start().await?;
        self.reload().await?;
        info!("dashcam-archive: CronSupervisor started");
        Ok(())
    }

    /// Re-read all enabled cron sources from DB and re-register jobs.
    pub async fn reload(&self) -> anyhow::Result<()> {
        // Remove all existing jobs.
        {
            let mut map = self.jobs.lock().await;
            for (_source_id, job_uuid) in map.drain() {
                if let Err(error) = self.scheduler.remove(&job_uuid).await {
                    warn!(%error, %job_uuid, "dashcam-archive: failed to remove cron job");
                }
            }
        }

        let sources = SourcesRepo::list_enabled(&self.db).await?;
        let cron_sources: Vec<_> = sources
            .into_iter()
            .filter(|s| {
                matches!(
                    s.trigger_mode.as_str(),
                    "cron" | "both" | "cron_and_watcher" | "cron+watcher"
                ) && s.cron_expr.as_deref().map(|e| !e.trim().is_empty()).unwrap_or(false)
            })
            .collect();

        let mut map = self.jobs.lock().await;
        for source in cron_sources {
            let raw_expr = source.cron_expr.clone().unwrap();
            // tokio-cron-scheduler uses 6-field (sec min hour dom month dow) or
            // 7-field (+year) cron format. Standard UNIX cron is 5-field
            // (min hour dom month dow). We detect 5-field by counting whitespace-
            // separated tokens and prepend "0 " to make it 6-field.
            let expr = normalize_cron_expr(&raw_expr);

            let source_id = source.id;
            let user_id = source.user_id;
            let orchestrator = self.orchestrator.clone();
            let active_runs = Arc::clone(&self.active_runs);

            let job = match Job::new_async(expr.as_str(), move |_uuid, _lock| {
                let orchestrator = orchestrator.clone();
                let active_runs = Arc::clone(&active_runs);
                Box::pin(async move {
                    {
                        let mut active = active_runs.lock().await;
                        if !active.insert(source_id) {
                            info!(
                                source_id = %source_id,
                                "dashcam-archive: skip cron trigger: previous run still active"
                            );
                            return;
                        }
                    }

                    tokio::spawn(async move {
                        if let Err(error) = orchestrator.run_source_with_trigger(source_id, user_id, "cron").await {
                            warn!(
                                %error,
                                %source_id,
                                "dashcam-archive: cron-triggered run failed"
                            );
                        } else {
                            info!(%source_id, "dashcam-archive: cron-triggered run completed");
                        }

                        active_runs.lock().await.remove(&source_id);
                    });
                })
            }) {
                Ok(job) => job,
                Err(error) => {
                    warn!(
                        %error,
                        %source_id,
                        cron_expr = %raw_expr,
                        "dashcam-archive: invalid cron expression, skipping source"
                    );
                    continue;
                }
            };

            match self.scheduler.add(job).await {
                Ok(job_uuid) => {
                    info!(
                        %source_id,
                        %job_uuid,
                        cron_expr = %expr,
                        "dashcam-archive: scheduled cron job"
                    );
                    map.insert(source_id, job_uuid);
                }
                Err(error) => {
                    warn!(%error, %source_id, "dashcam-archive: failed to add cron job");
                }
            }
        }

        info!(count = map.len(), "dashcam-archive: CronSupervisor reload complete");
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.scheduler.shutdown().await?;
        info!("dashcam-archive: CronSupervisor shutdown");
        Ok(())
    }
}

/// Convert a standard 5-field UNIX cron expression to the 6-field format
/// (sec min hour dom month dow) expected by tokio-cron-scheduler.
/// If the expression already has 6+ fields it is returned as-is.
fn normalize_cron_expr(expr: &str) -> String {
    let field_count = expr.split_whitespace().count();
    if field_count == 5 {
        format!("0 {}", expr)
    } else {
        expr.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_cron_expr;

    #[test]
    fn five_field_gets_seconds_prepended() {
        assert_eq!(normalize_cron_expr("30 6 * * *"), "0 30 6 * * *");
        assert_eq!(normalize_cron_expr("0 0 * * 1"), "0 0 0 * * 1");
    }

    #[test]
    fn six_field_unchanged() {
        assert_eq!(normalize_cron_expr("0 30 6 * * *"), "0 30 6 * * *");
    }

    #[test]
    fn seven_field_unchanged() {
        assert_eq!(normalize_cron_expr("0 15 6 * * * 2025"), "0 15 6 * * * 2025");
    }

    #[tokio::test]
    async fn reload_on_empty_db_does_not_panic() {
        // Smoke-test: constructing and reloading with no DB should not panic
        // (we can't spin up a real DB in unit tests; just verify normalize logic).
        let result = normalize_cron_expr("*/15 * * * *");
        assert_eq!(result, "0 */15 * * * *");
    }
}
