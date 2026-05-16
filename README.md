# tokimo-app-dashcam-archive

**视频分组合并与转码工具** — 行车记录仪、监控摄像头、流媒体档案的自动化管理与优化。支持多源文件扫描、按时间窗口分组、智能转码、定时任务、文件监视触发。

## 功能概览

### ✅ 已实现

| 功能 | 说明 |
|------|------|
| **多源输入** | 本地路径、SMB、SFTP、S3、FTP 等（via Tokimo VFS） |
| **智能分组** | 按时间窗口、摄像头关键字聚合视频，支持跨源合并 |
| **编码器选择** | Auto（自动判定）/ NVENC（硬加速）/ x265（软编码）/ Copy（直通） |
| **触发模式** | 手动触发、Cron 定时、文件监视、混合模式 |
| **转码控制** | 可配置帧率、码率、质量参数；支持断点续传 |
| **VFS DirectInput** | 直接从远程源读取、临时暂存、写回目标，无需本地中间文件 |
| **进度流** | SSE 实时报告扫描、转码、分组进度 |
| **历史记录** | 完整运行日志、统计数据（输入/输出大小、分组结果） |
| **文件缓存** | 扫描结果缓存（大小、修改时间、时长、编码信息） |

### 📋 计划中

- 主服务器任务队列集成（智能调度、资源竞争）
- 下载队列前端面板重构

## 架构

```
Browser (5173)
    │  /api/apps/dashcam-archive/*
    ▼
tokimo-server (5678)
    │  [transparent reverse proxy → UDS]
    ▼
$DATA_LOCAL_PATH/apps/dashcam-archive.sock
    │
tokimo-app-dashcam-archive
├─ Axum HTTP 服务器
│   ├─ /health              健康检查
│   ├─ /encoders            编码器列表
│   ├─ /sources             数据源 CRUD
│   ├─ /sources/{id}/run    触发合并
│   ├─ /sources/{id}/runs   历史查询
│   ├─ /runs/{id}           运行详情
│   ├─ /runs/{id}/stream    SSE 进度流
│   └─ /assets/*            前端资产
├─ 数据库（PostgreSQL，独立 schema）
├─ Cron 调度器
├─ 文件监视器（本地 + VFS 轮询）
├─ FFmpeg 流程管理（转码引擎）
└─ Tokimo Bus 总线客户端
```

## 数据模型

### sources

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 数据源唯一标识 |
| `user_id` | UUID | 所有者（多租户） |
| `name` | TEXT | 显示名称 |
| `src_source_id` | UUID | 源存储 ID（本地/SMB/SFTP/S3） |
| `src_source_type` | VARCHAR | 源类型（"local"、"smb"、"sftp" 等） |
| `src_path` | TEXT | 源路径 |
| `dst_source_id` | UUID | 目标存储 ID |
| `dst_source_type` | VARCHAR | 目标类型 |
| `dst_path` | TEXT | 目标路径 |
| `encoder` | TEXT | 编码器选择（"auto"、"nvenc"、"x265"、"copy"） |
| `encoder_params` | JSONB | 编码参数（码率、质量等） |
| `max_gap_seconds` | INT | 时间窗口最大间隔（秒） |
| `max_group_duration_seconds` | INT | 单组最长时长（0 = 无限） |
| `monthly_subdirs` | TEXT | 月份子目录规则（"auto"） |
| `trigger_mode` | TEXT | 触发模式（"manual_only"、"cron"、"watcher"、"cron_and_watcher"） |
| `cron_expr` | TEXT | Cron 表达式（5 或 6 字段） |
| `watcher_debounce_secs` | INT | 文件监视去抖延迟（秒） |
| `enabled` | BOOL | 是否启用 |
| `created_at`, `updated_at` | TIMESTAMPTZ | 时间戳 |

### scan_cache

缓存已扫描的文件元数据，避免重复探测。

| 字段 | 类型 | 说明 |
|------|------|------|
| `source_id` | UUID | 数据源 FK |
| `abs_path` | TEXT | 文件绝对路径 |
| `size`, `mtime_ns`, `ctime_ns` | - | 文件元信息 |
| `duration_secs` | DOUBLE | 时长（秒） |
| `codec`, `format_bps`, `width`, `height` | - | FFmpeg 探测结果 |
| `healthy`, `broken` | BOOL | 健康状态标志 |
| `probed_at` | TIMESTAMPTZ | 最后探测时间 |

### merge_runs

每次合并操作的完整历史。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 运行 ID |
| `source_id` | UUID | 数据源 FK |
| `trigger` | TEXT | 触发方式（"manual"、"cron"、"watcher"） |
| `status` | TEXT | 状态（"pending"、"running"、"ok"、"failed"、"cancelled"） |
| `started_at`, `finished_at` | TIMESTAMPTZ | 时间戳 |
| `total_groups`, `ok_groups`, `failed_groups` | INT | 统计计数 |
| `bytes_in`, `bytes_out` | BIGINT | 输入输出字节数 |
| `log_summary` | TEXT | 合并日志摘要 |

### merge_groups

运行内的分组记录（相同时间窗口的文件组）。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 组 ID |
| `run_id` | UUID | 运行 FK |
| `camera_key` | TEXT | 摄像头标识（从文件名提取） |
| `start_dt`, `end_dt` | TIMESTAMPTZ | 时间窗口 |
| `output_path` | TEXT | 输出文件路径 |
| `decision` | TEXT | 决策（"encode"、"copy"、"downgrade"） |
| `status` | TEXT | 状态（"ok"、"failed"） |
| `warning_level` | TEXT | 警告等级（"clean"、"warning"、"error"） |
| `bytes_in`, `bytes_out`, `duration_secs` | - | 统计 |

## HTTP API

### 健康检查

```http
GET /health
```

**响应**：
```json
{
  "status": "ok",
  "app": "dashcam-archive",
  "bus_bound": true,
  "ffmpeg_available": true
}
```

### 编码器列表

```http
GET /encoders
```

列出当前系统可用的编码器及其参数。

---

### 数据源 CRUD

#### 列表

```http
GET /sources
```

#### 创建

```http
POST /sources
Content-Type: application/json

{
  "name": "监控 A1",
  "src_source_id": "uuid",
  "src_source_type": "local",
  "src_path": "/mnt/camera/A1",
  "dst_source_id": "uuid",
  "dst_source_type": "local",
  "dst_path": "/mnt/archive/A1",
  "encoder": "auto",
  "max_gap_seconds": 60,
  "trigger_mode": "cron",
  "cron_expr": "0 2 * * *"
}
```

#### 查询

```http
GET /sources/{id}
```

#### 更新

```http
PATCH /sources/{id}
Content-Type: application/json

{ "encoder": "nvenc", "cron_expr": "0 2 * * *" }
```

#### 删除

```http
DELETE /sources/{id}
```

---

### 运行操作

#### 手动触发

```http
POST /sources/{id}/run
```

**响应**：
```json
{
  "run_id": "uuid",
  "status": "pending"
}
```

#### 列表历史

```http
GET /sources/{id}/runs
```

#### 获取运行详情

```http
GET /runs/{id}
```

#### 取消运行

```http
POST /runs/{id}/cancel
```

#### 流式进度

```http
GET /runs/{id}/stream

event: progress
data: {"run_id":"...","phase":"scan","group_count":5,"ok_count":3,"percent":60.0}

event: progress
data: {"run_id":"...","phase":"encode","current_file":"2024-01-15_12-30.mp4","percent":75.0}

event: done
data: {"status":"ok"}
```

## 触发模式

| 模式 | 说明 | 配置项 |
|------|------|--------|
| `manual_only` | 只支持手动触发（API POST） | 无 |
| `cron` | 定时任务触发 | `cron_expr`（5 或 6 字段 cron） |
| `watcher` | 文件监视触发 | `watcher_debounce_secs` |
| `cron_and_watcher` | 两种模式同时启用 | 两者均配置 |

**Cron 表达式兼容性**：
- 5 字段（标准 UNIX）：`min hour dom month dow` → 自动转为 6 字段 `0 min hour dom month dow`
- 6 字段（tokio-cron-scheduler）：`sec min hour dom month dow`

例：`0 2 * * *`（每天凌晨 2 点）

**开发验证**：设置 `cron_expr` 为 `*/2 * * * *`（每 2 分钟）可快速测试定时触发是否生效。

## 编码器选择策略

### Auto（自动）

推荐编码器，优先级：
1. **NVENC**（若可用且输入非 H.265）→ 硬加速，快速
2. **x265**（若输入非 H.265 且文件质量需要）→ 软编码，高效
3. **Copy**（若输入已是高质量格式）→ 直通，零转码开销

### NVENC（NVIDIA 硬加速）

- 依赖 NVIDIA GPU + CUDA
- 速度最快，功耗低
- 质量可配（CQ 参数）

### x265（软编码）

- 不需要 GPU
- 高效压缩，最小文件
- CPU 密集，速度最慢

### Copy（直通）

- 零转码，仅复制音视频流
- 最快，无质量损失
- 编码格式不变

## 开发指引

### 构建

#### Rust 后端

```bash
# 完整构建（嵌入前端资产）
cargo build --release

# 开发构建（读取 TOKIMO_APP_ASSETS_DIR）
export TOKIMO_APP_ASSETS_DIR=$(pwd)/ui/dist
cargo run
```

#### 前端（TypeScript + React）

```bash
cd ui
pnpm install
pnpm build        # 输出到 dist/
pnpm dev          # 开发服务器（Vite HMR）
```

### 环境配置

| 变量 | 说明 | 示例 |
|------|------|------|
| `DATABASE_URL` | PostgreSQL 连接字符串 | `postgres://user:pass@localhost/tokimo_db` |
| `TOKIMO_DATA_DIR` | 临时状态目录 | `/data/tokimo` |
| `TOKIMO_APP_ASSETS_DIR` | 前端资产路径（开发用） | `./ui/dist` |
| `LOG_LEVEL` | 日志级别 | `info`、`debug`、`trace` |
| `RUST_BACKTRACE` | 栈跟踪 | `1`（启用） |

### 测试 API

```bash
# 健康检查
curl http://localhost:5678/api/apps/dashcam-archive/health

# 列表编码器
curl http://localhost:5678/api/apps/dashcam-archive/encoders

# 创建数据源
curl -X POST http://localhost:5678/api/apps/dashcam-archive/sources \
  -H "Content-Type: application/json" \
  -d '{...}'
```

## 已知限制

1. **文件 > 100GB**：暂未针对超大文件进行性能优化（内存占用可能较高）
2. **并发转码**：默认 4 worker，可配置但受 CPU/GPU 限制
3. **网络源稳定性**：SFTP/SMB 连接中断会导致运行失败，需手动重试
4. **时长探测**：某些格式可能无法准确获取时长，缓存后无法更新
5. **Cron 微秒精度**：最小间隔为秒级，不支持毫秒级任务
6. **文件监视去抖**：最小 30 秒，防止频繁触发；可通过 `watcher_debounce_secs` 调整

## License

MIT OR Apache-2.0
