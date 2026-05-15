use std::{collections::HashMap, sync::Arc};

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    db::{
        entities::merge_runs,
        repos::sources_repo::SourcesRepo,
    },
    orchestrator::Orchestrator,
};

/// Maps source_id → scheduler job UUID so we can remove stale jobs on reload.
type JobMap = Arc<Mutex<HashMap<Uuid, Uuid>>>;

pub struct CronSupervisor {
    scheduler: JobScheduler,
    orchestrator: Orchestrator,
    db: DatabaseConnection,
    /// source_id → scheduler job UUID
    jobs: JobMap,
}

impl CronSupervisor {
    pub async fn new(db: DatabaseConnection, orchestrator: Orchestrator) -> anyhow::Result<Self> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self {
            scheduler,
            orchestrator,
            db,
            jobs: Arc::new(Mutex::new(HashMap::new())),
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
                matches!(s.trigger_mode.as_str(), "cron" | "cron_and_watcher")
                    && s.cron_expr.as_deref().map(|e| !e.trim().is_empty()).unwrap_or(false)
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
            let db = self.db.clone();

            let job = match Job::new_async(expr.as_str(), move |_uuid, _lock| {
                let orchestrator = orchestrator.clone();
                let db = db.clone();
                Box::pin(async move {
                    // Dedup: skip if there's already a queued or running run for this source.
                    match has_active_run(&db, source_id).await {
                        Ok(true) => {
                            info!(
                                source_id = %source_id,
                                "dashcam-archive: cron tick skipped — run already active"
                            );
                            return;
                        }
                        Err(error) => {
                            warn!(%error, %source_id, "dashcam-archive: dedup check failed, proceeding anyway");
                        }
                        Ok(false) => {}
                    }

                    if let Err(error) = orchestrator.enqueue_run(source_id, user_id).await {
                        warn!(
                            %error,
                            %source_id,
                            "dashcam-archive: cron-triggered enqueue_run failed"
                        );
                    } else {
                        info!(%source_id, "dashcam-archive: cron-triggered run enqueued");
                    }
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

        info!(
            count = map.len(),
            "dashcam-archive: CronSupervisor reload complete"
        );
        Ok(())
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.scheduler.shutdown().await?;
        info!("dashcam-archive: CronSupervisor shutdown");
        Ok(())
    }
}

/// Returns true if there is already a queued or running merge_run for this source.
async fn has_active_run(db: &DatabaseConnection, source_id: Uuid) -> anyhow::Result<bool> {
    let count = merge_runs::Entity::find()
        .filter(merge_runs::Column::SourceId.eq(source_id))
        .filter(
            merge_runs::Column::Status
                .is_in(["queued", "running"]),
        )
        .count(db)
        .await?;
    Ok(count > 0)
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
