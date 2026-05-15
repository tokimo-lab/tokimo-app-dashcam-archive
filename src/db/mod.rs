use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

pub mod entities;
pub mod repos;

const SCHEMA: &str = "dashcam_archive";

pub async fn init_pool() -> anyhow::Result<DatabaseConnection> {
    let base_url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
    let sep = if base_url.contains('?') { '&' } else { '?' };
    let url = format!("{base_url}{sep}application_name=tokimo-app-dashcam-archive");
    let mut opts = ConnectOptions::new(url);
    opts.max_connections(4).min_connections(1).sqlx_logging(false);
    Ok(Database::connect(opts).await?)
}

pub async fn init_schema(db: &DatabaseConnection) -> anyhow::Result<()> {
    let ddl = [
        format!(r#"CREATE SCHEMA IF NOT EXISTS {SCHEMA}"#),
        format!(
            r#"CREATE TABLE IF NOT EXISTS {SCHEMA}.sources (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id UUID NOT NULL,
                name TEXT NOT NULL,
                src_source_id UUID NOT NULL,
                src_source_type VARCHAR(32) NOT NULL,
                dst_source_id UUID NOT NULL,
                dst_source_type VARCHAR(32) NOT NULL,
                src_path TEXT NOT NULL,
                dst_path TEXT NOT NULL,
                encoder TEXT NOT NULL DEFAULT 'auto',
                encoder_params JSONB NOT NULL DEFAULT '{{}}',
                max_gap_seconds INT NOT NULL DEFAULT 60,
                max_group_duration_seconds INT NOT NULL DEFAULT 0,
                monthly_subdirs TEXT NOT NULL DEFAULT 'auto',
                allow_combined_input BOOL NOT NULL DEFAULT false,
                no_broken_split BOOL NOT NULL DEFAULT false,
                trigger_mode TEXT NOT NULL DEFAULT 'manual_only',
                cron_expr TEXT,
                watcher_debounce_secs INT NOT NULL DEFAULT 60,
                enabled BOOL NOT NULL DEFAULT true,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"#
        ),
        format!(
            r#"CREATE TABLE IF NOT EXISTS {SCHEMA}.scan_cache (
                source_id UUID REFERENCES {SCHEMA}.sources(id) ON DELETE CASCADE,
                abs_path TEXT NOT NULL,
                size BIGINT,
                mtime_ns BIGINT,
                ctime_ns BIGINT,
                duration_secs DOUBLE PRECISION,
                healthy BOOL NOT NULL DEFAULT true,
                broken BOOL NOT NULL DEFAULT false,
                probed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY(source_id, abs_path)
            )"#
        ),
        format!(r#"ALTER TABLE {SCHEMA}.scan_cache ADD COLUMN IF NOT EXISTS codec TEXT"#),
        format!(r#"ALTER TABLE {SCHEMA}.scan_cache ADD COLUMN IF NOT EXISTS format_bps BIGINT"#),
        format!(r#"ALTER TABLE {SCHEMA}.scan_cache ADD COLUMN IF NOT EXISTS size_bytes BIGINT"#),
        format!(r#"ALTER TABLE {SCHEMA}.scan_cache ADD COLUMN IF NOT EXISTS width INT"#),
        format!(r#"ALTER TABLE {SCHEMA}.scan_cache ADD COLUMN IF NOT EXISTS height INT"#),
        format!(
            r#"CREATE TABLE IF NOT EXISTS {SCHEMA}.merge_runs (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                source_id UUID REFERENCES {SCHEMA}.sources(id) ON DELETE CASCADE,
                trigger TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                finished_at TIMESTAMPTZ,
                total_groups INT NOT NULL DEFAULT 0,
                ok_groups INT NOT NULL DEFAULT 0,
                downgraded_groups INT NOT NULL DEFAULT 0,
                failed_groups INT NOT NULL DEFAULT 0,
                bytes_in BIGINT,
                bytes_out BIGINT,
                folder_breaker_tripped BOOL NOT NULL DEFAULT false,
                log_summary TEXT
            )"#
        ),
        format!(
            r#"CREATE TABLE IF NOT EXISTS {SCHEMA}.merge_groups (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                run_id UUID REFERENCES {SCHEMA}.merge_runs(id) ON DELETE CASCADE,
                camera_key TEXT NOT NULL,
                start_dt TIMESTAMPTZ,
                end_dt TIMESTAMPTZ,
                output_path TEXT NOT NULL,
                decision TEXT NOT NULL,
                status TEXT NOT NULL,
                warning_level TEXT NOT NULL DEFAULT 'clean',
                bytes_in BIGINT,
                bytes_out BIGINT,
                duration_secs DOUBLE PRECISION,
                abort_reason TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"#
        ),
        format!(
            r#"CREATE TABLE IF NOT EXISTS {SCHEMA}.warnings (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                group_id UUID REFERENCES {SCHEMA}.merge_groups(id) ON DELETE CASCADE,
                warning_key TEXT NOT NULL,
                count INT NOT NULL DEFAULT 1,
                first_example TEXT
            )"#
        ),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid()"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS source_id UUID"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS trigger TEXT NOT NULL DEFAULT ''"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'queued'"#),
        format!(
            r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ NOT NULL DEFAULT NOW()"#
        ),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS finished_at TIMESTAMPTZ"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS total_groups INT NOT NULL DEFAULT 0"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS ok_groups INT NOT NULL DEFAULT 0"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS downgraded_groups INT NOT NULL DEFAULT 0"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS failed_groups INT NOT NULL DEFAULT 0"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS bytes_in BIGINT"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS bytes_out BIGINT"#),
        format!(
            r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS folder_breaker_tripped BOOL NOT NULL DEFAULT false"#
        ),
        format!(r#"ALTER TABLE {SCHEMA}.merge_runs ADD COLUMN IF NOT EXISTS log_summary TEXT"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid()"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS run_id UUID"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS camera_key TEXT NOT NULL DEFAULT ''"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS start_dt TIMESTAMPTZ"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS end_dt TIMESTAMPTZ"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS output_path TEXT NOT NULL DEFAULT ''"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS decision TEXT NOT NULL DEFAULT 'copy'"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'ok'"#),
        format!(
            r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS warning_level TEXT NOT NULL DEFAULT 'clean'"#
        ),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS bytes_in BIGINT"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS bytes_out BIGINT"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS duration_secs DOUBLE PRECISION"#),
        format!(r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS abort_reason TEXT"#),
        format!(
            r#"ALTER TABLE {SCHEMA}.merge_groups ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()"#
        ),
        format!(r#"ALTER TABLE {SCHEMA}.warnings ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid()"#),
        format!(r#"ALTER TABLE {SCHEMA}.warnings ADD COLUMN IF NOT EXISTS group_id UUID"#),
        format!(
            r#"ALTER TABLE {SCHEMA}.warnings ADD COLUMN IF NOT EXISTS warning_key TEXT NOT NULL DEFAULT 'unknown'"#
        ),
        format!(r#"ALTER TABLE {SCHEMA}.warnings ADD COLUMN IF NOT EXISTS count INT NOT NULL DEFAULT 0"#),
        format!(r#"ALTER TABLE {SCHEMA}.warnings ADD COLUMN IF NOT EXISTS first_example TEXT"#),
        format!(r#"CREATE INDEX IF NOT EXISTS sources_user_id_idx ON {SCHEMA}.sources (user_id)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS sources_enabled_idx ON {SCHEMA}.sources (enabled)"#),
        format!(
            r#"CREATE INDEX IF NOT EXISTS merge_runs_source_started_idx ON {SCHEMA}.merge_runs (source_id, started_at DESC)"#
        ),
        format!(r#"CREATE INDEX IF NOT EXISTS merge_groups_run_idx ON {SCHEMA}.merge_groups (run_id)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS warnings_group_idx ON {SCHEMA}.warnings (group_id)"#),
    ];
    for sql in ddl {
        db.execute_raw(Statement::from_string(DatabaseBackend::Postgres, sql))
            .await?;
    }
    Ok(())
}
