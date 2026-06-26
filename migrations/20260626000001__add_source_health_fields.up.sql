-- Add source-level health/preflight options introduced after the initial host-managed migration.

ALTER TABLE sources
    ADD COLUMN IF NOT EXISTS preflight_bitrate_ref INT NOT NULL DEFAULT 5000000;

ALTER TABLE sources
    ADD COLUMN IF NOT EXISTS hybrid_health_check BOOL NOT NULL DEFAULT true;
