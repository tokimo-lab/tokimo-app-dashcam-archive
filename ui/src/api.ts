/**
 * API client for dashcam-archive backend.
 * All types and fetch helpers for /api/apps/dashcam-archive endpoints.
 */

// ── DTOs ───────────────────────────────────────────────────────────────────

export interface EncoderDto {
  id: string;
  display_name: string;
  description: string;
  available: boolean;
  args: string[];
  supports_h265: boolean;
}

export interface SourceReq {
  name: string;
  src_path?: string;
  dst_path?: string;
  encoder?: string;
  encoder_params?: Record<string, unknown>;
  max_gap_seconds?: number;
  max_group_duration_seconds?: number;
  monthly_subdirs?: "auto" | "on" | "off";
  allow_combined_input?: boolean;
  no_broken_split?: boolean;
  trigger_mode?: "manual_only" | "cron" | "watcher" | "cron+watcher";
  cron_expr?: string;
  watcher_debounce_secs?: number;
  enabled?: boolean;
}

export interface SourcePatchReq extends Partial<SourceReq> {}

export interface SourceDto {
  id: string;
  user_id: string;
  name: string;
  src_path: string | null;
  dst_path: string | null;
  encoder: string;
  encoder_params: Record<string, unknown>;
  max_gap_seconds: number;
  max_group_duration_seconds: number;
  monthly_subdirs: "auto" | "on" | "off";
  allow_combined_input: boolean;
  no_broken_split: boolean;
  trigger_mode: "manual_only" | "cron" | "watcher" | "cron+watcher";
  cron_expr: string | null;
  watcher_debounce_secs: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface RunDto {
  id: string;
  source_id: string;
  trigger: "manual" | "cron" | "watcher";
  status: "queued" | "running" | "succeeded" | "failed" | "cancelled";
  started_at: string;
  finished_at: string | null;
  total_groups: number;
  ok_groups: number;
  downgraded_groups: number;
  failed_groups: number;
  bytes_in: number | null;
  bytes_out: number | null;
  folder_breaker_tripped: boolean;
  log_summary: string | null;
}

export interface GroupDto {
  id: string;
  camera_key: string;
  start_dt: string | null;
  end_dt: string | null;
  output_path: string;
  decision: string;
  status: string;
  warning_level: "clean" | "warn" | "suspicious" | "fatal";
  duration_secs: number | null;
  bytes_in: number | null;
  bytes_out: number | null;
  abort_reason: string | null;
}

export interface WarningDto {
  id: string;
  group_id: string;
  warning_key: string;
  count: number;
  first_example: string | null;
}

export interface RunDetailDto {
  run: RunDto;
  groups: GroupDto[];
  warnings: WarningDto[];
}

export interface RunStartResponse {
  run_id: string;
}

export interface ProgressEvent {
  phase: string;
  group_count: number;
  ok_count: number;
  failed_count: number;
  current_file: string | null;
  percent: number;
}

// ── Fetch helpers ──────────────────────────────────────────────────────────

const BASE_URL = "/api/apps/dashcam-archive";

async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, {
    ...init,
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      ...init?.headers,
    },
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`HTTP ${res.status}: ${text}`);
  }
  if (res.status === 204) {
    return undefined as T;
  }
  const text = await res.text();
  if (!text) {
    return undefined as T;
  }
  return JSON.parse(text) as T;
}

// ── API functions ──────────────────────────────────────────────────────────

export async function getEncoders(): Promise<EncoderDto[]> {
  return fetchJson<EncoderDto[]>("/encoders");
}

export async function getSources(): Promise<SourceDto[]> {
  return fetchJson<SourceDto[]>("/sources");
}

export async function getSource(id: string): Promise<SourceDto> {
  return fetchJson<SourceDto>(`/sources/${encodeURIComponent(id)}`);
}

export async function createSource(req: SourceReq): Promise<SourceDto> {
  return fetchJson<SourceDto>("/sources", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateSource(
  id: string,
  req: SourcePatchReq,
): Promise<SourceDto> {
  return fetchJson<SourceDto>(`/sources/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteSource(id: string): Promise<void> {
  await fetchJson<void>(`/sources/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export async function runSource(id: string): Promise<RunStartResponse> {
  return fetchJson<RunStartResponse>(`/sources/${encodeURIComponent(id)}/run`, {
    method: "POST",
  });
}

export async function getSourceRuns(id: string, limit = 50): Promise<RunDto[]> {
  return fetchJson<RunDto[]>(
    `/sources/${encodeURIComponent(id)}/runs?limit=${limit}`,
  );
}

export async function getRunDetail(id: string): Promise<RunDetailDto> {
  return fetchJson<RunDetailDto>(`/runs/${encodeURIComponent(id)}`);
}

export async function cancelRun(id: string): Promise<void> {
  await fetchJson<void>(`/runs/${encodeURIComponent(id)}/cancel`, {
    method: "POST",
  });
}

export function subscribeRunProgress(
  runId: string,
  onMessage: (evt: ProgressEvent) => void,
  onError?: (err: Error) => void,
): () => void {
  const url = `${BASE_URL}/runs/${encodeURIComponent(runId)}/stream`;
  const es = new EventSource(url);

  es.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data) as ProgressEvent;
      onMessage(data);
    } catch (err) {
      onError?.(
        err instanceof Error ? err : new Error("Failed to parse SSE data"),
      );
    }
  };

  es.onerror = () => {
    onError?.(new Error("SSE connection error"));
    es.close();
  };

  return () => es.close();
}

// ── Utilities ──────────────────────────────────────────────────────────────

export interface FormatLabels {
  empty: string;
  byteUnits: readonly string[];
  unitValueSeparator: string;
  secondUnit: string;
  minuteUnit: string;
  hourUnit: string;
  durationPartsSeparator: string;
}

export function formatBytes(
  bytes: number | null | undefined,
  labels: Pick<FormatLabels, "empty" | "byteUnits" | "unitValueSeparator">,
): string {
  if (bytes == null) return labels.empty;
  const firstUnit = labels.byteUnits[0] ?? "";
  if (bytes === 0) return `0${labels.unitValueSeparator}${firstUnit}`;
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const unit =
    labels.byteUnits[i] ??
    labels.byteUnits[labels.byteUnits.length - 1] ??
    firstUnit;
  return `${(bytes / k ** i).toFixed(1)}${labels.unitValueSeparator}${unit}`;
}

export function formatDuration(
  seconds: number | null | undefined,
  labels: Pick<
    FormatLabels,
    | "empty"
    | "secondUnit"
    | "minuteUnit"
    | "hourUnit"
    | "durationPartsSeparator"
  >,
): string {
  if (seconds == null) return labels.empty;
  if (seconds < 60) return `${seconds}${labels.secondUnit}`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m < 60) {
    return `${m}${labels.minuteUnit}${labels.durationPartsSeparator}${s}${labels.secondUnit}`;
  }
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return `${h}${labels.hourUnit}${labels.durationPartsSeparator}${rm}${labels.minuteUnit}`;
}
