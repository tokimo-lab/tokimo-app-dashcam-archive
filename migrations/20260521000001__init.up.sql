-- Initial schema for dashcam-archive app.
-- Schema name + search_path are injected by host (TOKIMO_APP_SCHEMA env).
-- No CREATE SCHEMA, no schema prefix, no IF NOT EXISTS — host ledger handles idempotency.

CREATE TABLE sources (
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
    encoder_params JSONB NOT NULL DEFAULT '{}',
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
);

CREATE TABLE scan_cache (
    source_id UUID REFERENCES sources(id) ON DELETE CASCADE,
    abs_path TEXT NOT NULL,
    size BIGINT,
    mtime_ns BIGINT,
    ctime_ns BIGINT,
    duration_secs DOUBLE PRECISION,
    healthy BOOL NOT NULL DEFAULT true,
    broken BOOL NOT NULL DEFAULT false,
    probed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    codec TEXT,
    format_bps BIGINT,
    size_bytes BIGINT,
    width INT,
    height INT,
    PRIMARY KEY (source_id, abs_path)
);

CREATE TABLE merge_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID REFERENCES sources(id) ON DELETE CASCADE,
    trigger TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'queued',
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
);

CREATE TABLE merge_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID REFERENCES merge_runs(id) ON DELETE CASCADE,
    camera_key TEXT NOT NULL DEFAULT '',
    start_dt TIMESTAMPTZ,
    end_dt TIMESTAMPTZ,
    output_path TEXT NOT NULL DEFAULT '',
    decision TEXT NOT NULL DEFAULT 'copy',
    status TEXT NOT NULL DEFAULT 'ok',
    warning_level TEXT NOT NULL DEFAULT 'clean',
    bytes_in BIGINT,
    bytes_out BIGINT,
    duration_secs DOUBLE PRECISION,
    abort_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE warnings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    group_id UUID REFERENCES merge_groups(id) ON DELETE CASCADE,
    warning_key TEXT NOT NULL DEFAULT 'unknown',
    count INT NOT NULL DEFAULT 1,
    first_example TEXT
);

CREATE INDEX sources_user_id_idx ON sources (user_id);
CREATE INDEX sources_enabled_idx ON sources (enabled);
CREATE INDEX merge_runs_source_started_idx ON merge_runs (source_id, started_at DESC);
CREATE INDEX merge_groups_run_idx ON merge_groups (run_id);
CREATE INDEX warnings_group_idx ON warnings (group_id);
