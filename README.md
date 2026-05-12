# tokimo-app-dashcam-archive

录像归并 — Tokimo standalone multi-process app scaffold for 行车记录仪 / 监控视频按时间分组合并与转码.

## Architecture

```
Browser
  │  /api/apps/dashcam-archive/<route>
  ▼
tokimo-server (5678)
  │  transparent reverse proxy → UDS
  ▼
$DATA_LOCAL_PATH/apps/dashcam-archive.sock
  │
this binary
  ├─ axum router (src/app_server.rs)
  │   ├─ GET /health
  │   └─ GET /assets/{*path}
  ├─ tokimo-bus client
  └─ DB scaffold only (no app tables yet)
```

## Current scaffold

- App id / window type: `dashcam-archive`
- Display name: `录像归并`
- Manifest category: `app`
- Health endpoint: `GET /health` returns `{ "status": "ok", "app": "dashcam-archive" }`
- UI entry renders: `录像归并 (dashcam-archive) — scaffold ready`

## Local development

Build UI assets from `ui/` when needed; the Rust binary embeds `ui/dist` for packaged runs and can read `TOKIMO_APP_ASSETS_DIR` in dev mode.

## License

MIT OR Apache-2.0.
