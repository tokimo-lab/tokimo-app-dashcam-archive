/**
 * Dashboard card for a single dashcam archive source.
 * Manages its own latest-run state and SSE subscription.
 */
import type { ShellApi } from "@tokimo/sdk";
import { Button, Progress, Switch, Tag, useToast } from "@tokimo/ui";
import { History, Play, Settings, X } from "lucide-react";
import type { MouseEvent } from "react";
import { useEffect, useRef, useState } from "react";
import type { FormatLabels, ProgressEvent, RunDto, SourceDto } from "./api";
import {
  cancelRun,
  formatBytes,
  formatDuration,
  getSourceRuns,
  runSource,
  subscribeRunProgress,
  updateSource,
} from "./api";
import { registerBridge } from "./modal-bridge";

interface Props {
  source: SourceDto;
  onSettingsClick: () => void;
  onToggle: (enabled: boolean) => void;
  onViewHistory: () => void;
  t: (key: string) => string;
  shell: ShellApi;
  locale: string;
}

function pathSummary(source: SourceDto): string {
  const src = source.src_source_name
    ? `${source.src_source_name}::${source.src_path ?? ""}`
    : (source.src_path ?? "");
  const dst = source.dst_source_name
    ? `${source.dst_source_name}::${source.dst_path ?? ""}`
    : (source.dst_path ?? "");
  return `${src} → ${dst}`;
}

function statusKey(status: RunDto["status"]): string {
  switch (status) {
    case "running":
      return "cardStatusRunning";
    case "queued":
      return "cardStatusQueued";
    case "succeeded":
      return "cardStatusSucceeded";
    case "failed":
      return "cardStatusFailed";
    case "cancelled":
      return "cardStatusCancelled";
    default:
      return "cardStatusIdle";
  }
}

function statusColor(
  status: RunDto["status"],
): "success" | "error" | "processing" | "warning" | "default" {
  switch (status) {
    case "succeeded":
      return "success";
    case "failed":
      return "error";
    case "running":
      return "processing";
    case "queued":
      return "warning";
    default:
      return "default";
  }
}

function relativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return "< 1m";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

function closestRectFromEvent(
  event: MouseEvent<HTMLElement>,
  selector: string,
): DOMRect | null {
  const currentMatch = event.currentTarget.closest(selector);
  if (currentMatch) return currentMatch.getBoundingClientRect();

  if (event.target instanceof Element) {
    const targetMatch = event.target.closest(selector);
    if (targetMatch) return targetMatch.getBoundingClientRect();
  }

  return null;
}

function getDryRunModalSize(event: MouseEvent<HTMLElement>): {
  width: number;
  height: number;
} {
  const parentRect =
    closestRectFromEvent(event, "[data-window-id]") ??
    closestRectFromEvent(event, "[data-third-party-app]");
  const parentWidth = parentRect?.width ?? window.innerWidth;
  const parentHeight = parentRect?.height ?? window.innerHeight;

  return {
    width: Math.min(parentWidth * 0.9, 600),
    height: Math.min(parentHeight * 0.9, 800),
  };
}

function etaSeconds(startedAt: string, percent: number): number | null {
  if (percent <= 0) return null;
  const elapsed = (Date.now() - new Date(startedAt).getTime()) / 1000;
  return Math.round((elapsed / percent) * (100 - percent));
}

export function SourceCard({
  source,
  onSettingsClick,
  onToggle,
  onViewHistory,
  t,
  shell,
  locale,
}: Props) {
  const toast = useToast();
  const [latestRun, setLatestRun] = useState<RunDto | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [runStartedAt, setRunStartedAt] = useState<string | null>(null);
  const [toggling, setToggling] = useState(false);
  const [enabled, setEnabled] = useState(source.enabled);
  const unsubRef = useRef<(() => void) | null>(null);

  const formatLabels: FormatLabels = {
    empty: t("emptyValue"),
    byteUnits: [
      t("unitByte"),
      t("unitKilobyte"),
      t("unitMegabyte"),
      t("unitGigabyte"),
      t("unitTerabyte"),
    ],
    unitValueSeparator: " ",
    secondUnit: t("unitSecondShort"),
    minuteUnit: t("unitMinuteShort"),
    hourUnit: t("unitHourShort"),
    durationPartsSeparator: " ",
  };

  useEffect(() => {
    let mounted = true;
    getSourceRuns(source.id, 1)
      .then((runs) => {
        if (!mounted) return;
        const run = runs[0] ?? null;
        setLatestRun(run);
        if (run?.status === "running" || run?.status === "queued") {
          setActiveRunId(run.id);
          setRunStartedAt(run.started_at);
        }
      })
      .catch(() => {});
    return () => {
      mounted = false;
    };
  }, [source.id]);

  useEffect(() => {
    if (!activeRunId) return;
    unsubRef.current?.();
    const unsub = subscribeRunProgress(
      activeRunId,
      (evt) => {
        setProgress(evt);
        if (evt.percent >= 100) {
          setActiveRunId(null);
          setLatestRun((prev) =>
            prev ? { ...prev, status: "succeeded" } : prev,
          );
        }
      },
      () => {
        setActiveRunId(null);
      },
    );
    unsubRef.current = unsub;
    return () => {
      unsub();
      unsubRef.current = null;
    };
  }, [activeRunId]);

  const isRunning =
    activeRunId !== null ||
    latestRun?.status === "running" ||
    latestRun?.status === "queued";

  const handleRunNow = async () => {
    try {
      const res = await runSource(source.id);
      const now = new Date().toISOString();
      setActiveRunId(res.run_id);
      setRunStartedAt(now);
      setProgress(null);
      setLatestRun((prev) =>
        prev
          ? { ...prev, id: res.run_id, status: "running", started_at: now }
          : ({
              id: res.run_id,
              source_id: source.id,
              trigger: "manual",
              status: "running",
              started_at: now,
              finished_at: null,
              total_groups: 0,
              ok_groups: 0,
              downgraded_groups: 0,
              failed_groups: 0,
              bytes_in: null,
              bytes_out: null,
              folder_breaker_tripped: false,
              log_summary: null,
            } as RunDto),
      );
    } catch (err) {
      toast.error(
        `${t("errorRun")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  };

  const handleCancel = async () => {
    if (!activeRunId) return;
    try {
      await cancelRun(activeRunId);
      setActiveRunId(null);
      setProgress(null);
      setLatestRun((prev) => (prev ? { ...prev, status: "cancelled" } : prev));
    } catch (err) {
      toast.error(
        `${t("errorCancel")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  };

  const handleToggle = async (val: boolean) => {
    setToggling(true);
    setEnabled(val);
    try {
      await updateSource(source.id, { enabled: val });
      onToggle(val);
    } catch (err) {
      setEnabled(!val);
      toast.error(
        `${t("errorSave")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    } finally {
      setToggling(false);
    }
  };

  const handleDryRun = (event: MouseEvent<HTMLElement>) => {
    const { width, height } = getDryRunModalSize(event);
    const bridgeId = registerBridge({
      kind: "dry-run",
      sourceId: source.id,
      dstPath: source.dst_path,
      onLoadingChange: () => {},
    });
    shell.openModalWindow({
      component: () => import("./dry-run-modal-window"),
      title: "Dry Run 计划",
      width,
      height,
      metadata: { bridgeId, locale },
    });
  };

  const runStatus = isRunning ? "running" : (latestRun?.status ?? null);
  const percent = progress?.percent ?? 0;
  const eta =
    isRunning && runStartedAt ? etaSeconds(runStartedAt, percent) : null;
  const etaLabel = eta != null ? formatDuration(eta, formatLabels) : null;
  const savedBytes =
    latestRun?.bytes_in != null && latestRun?.bytes_out != null
      ? latestRun.bytes_in - latestRun.bytes_out
      : null;

  return (
    <div
      className={`
        bg-surface-raised border-border-base flex flex-col rounded-xl border
        shadow-sm transition-shadow hover:shadow-md
        ${!enabled ? "opacity-60" : ""}
      `}
    >
      {/* Card header */}
      <div className="flex items-start justify-between gap-2 p-4 pb-2">
        <div className="min-w-0 flex-1">
          <div className="text-fg-primary truncate text-base font-semibold">
            {source.name}
          </div>
          <div className="text-fg-muted mt-0.5 truncate text-xs font-mono">
            {pathSummary(source)}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button
            type="button"
            onClick={onSettingsClick}
            className="text-fg-secondary hover:text-fg-primary cursor-pointer rounded p-1 transition-colors hover:bg-surface-overlay"
            title={t("modalEditSource")}
          >
            <Settings size={16} />
          </button>
          <Switch
            checked={enabled}
            onChange={handleToggle}
            disabled={toggling}
          />
        </div>
      </div>

      {/* Status & progress */}
      <div className="flex-1 px-4 py-3">
        {runStatus ? (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <Tag color={statusColor(runStatus as RunDto["status"])}>
                {t(statusKey(runStatus as RunDto["status"]))}
              </Tag>
              {isRunning && etaLabel && (
                <span className="text-fg-muted text-xs">
                  {t("cardEta")} {etaLabel}
                </span>
              )}
              {!isRunning && latestRun?.started_at && (
                <span className="text-fg-muted text-xs">
                  {t("cardLastRun")} {relativeTime(latestRun.started_at)}
                </span>
              )}
            </div>
            {isRunning && (
              <div className="space-y-1">
                <Progress
                  percent={Math.round(percent)}
                  status={runStatus === "failed" ? "exception" : "normal"}
                  showInfo
                  strokeWidth={6}
                />
                {progress && (
                  <div className="text-fg-muted text-xs">
                    {progress.ok_count}/{progress.group_count}{" "}
                    {t("cardGroupsDone")}
                    {progress.current_file && (
                      <span className="ml-2 font-mono truncate block">
                        {progress.current_file}
                      </span>
                    )}
                  </div>
                )}
              </div>
            )}
            {!isRunning && savedBytes != null && savedBytes > 0 && (
              <div className="text-fg-secondary text-xs">
                {t("separatorFlow")} {formatBytes(savedBytes, formatLabels)}{" "}
                saved
              </div>
            )}
          </div>
        ) : (
          <span className="text-fg-muted text-sm">{t("cardNoRuns")}</span>
        )}
      </div>

      {/* Card footer */}
      <div className="border-border-subtle flex items-center gap-2 border-t px-4 py-3">
        {isRunning ? (
          <Button
            size="small"
            variant="danger"
            onClick={handleCancel}
            className="flex items-center gap-1"
          >
            <X size={14} />
            {t("cardCancelRun")}
          </Button>
        ) : (
          <>
            <Button
              size="small"
              onClick={handleRunNow}
              disabled={!enabled}
              className="flex items-center gap-1"
            >
              <Play size={14} />
              {t("cardRunNow")}
            </Button>
            <Button
              size="small"
              variant="dashed"
              onClick={handleDryRun}
              disabled={!enabled}
              className="flex items-center gap-1"
            >
              模拟运行
            </Button>
          </>
        )}
        <Button
          size="small"
          variant="secondary"
          onClick={onViewHistory}
          className="flex items-center gap-1"
        >
          <History size={14} />
          {t("cardViewHistory")}
        </Button>
      </div>
    </div>
  );
}
