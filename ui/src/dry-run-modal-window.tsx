import { makeTranslator, type ShellWindowHandle } from "@tokimo/sdk";
import { Button, Spin } from "@tokimo/ui";
import { useEffect, useMemo, useState } from "react";
import type { DryRunPlan, FormatLabels } from "./api";
import { dryRun, formatBytes, formatDuration } from "./api";
import { enUS, zhCN } from "./i18n";
import { getBridge } from "./modal-bridge";

const PAGE_SIZE = 10;

function pad2(value: number): string {
  return value.toString().padStart(2, "0");
}

function formatDatePart(date: Date): string {
  return `${date.getFullYear()}/${pad2(date.getMonth() + 1)}/${pad2(date.getDate())}`;
}

function formatTimePart(date: Date): string {
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`;
}

function formatTimeRange(
  startAt: string | null,
  endAt: string | null,
): string | null {
  if (!startAt) return null;
  const start = new Date(startAt);
  const end = new Date(endAt ?? startAt);
  if (Number.isNaN(start.getTime()) || Number.isNaN(end.getTime())) return null;
  const startDate = formatDatePart(start);
  const endDate = formatDatePart(end);
  if (startDate === endDate) {
    return `${startDate} ${formatTimePart(start)} → ${formatTimePart(end)}`;
  }
  return `${startDate} ${formatTimePart(start)} → ${endDate} ${formatTimePart(end)}`;
}

export default function DryRunModalWindow({ win }: { win: ShellWindowHandle }) {
  const bridgeId =
    typeof win.metadata.bridgeId === "string" ? win.metadata.bridgeId : "";
  const locale =
    typeof win.metadata.locale === "string" ? win.metadata.locale : "zh-CN";
  const t = useMemo(
    () => makeTranslator({ "zh-CN": zhCN, "en-US": enUS }, locale),
    [locale],
  );
  const [bridge] = useState(() => (bridgeId ? getBridge(bridgeId) : undefined));
  const [plan, setPlan] = useState<DryRunPlan | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<number>>(new Set());
  const [page, setPage] = useState(1);

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
    if (bridge?.kind !== "dry-run") return;
    let mounted = true;
    bridge.onLoadingChange(true);
    setLoading(true);
    setError(null);
    dryRun(bridge.sourceId)
      .then((nextPlan) => {
        if (!mounted) return;
        setPlan(nextPlan);
        setPage(1);
      })
      .catch((err) => {
        if (!mounted) return;
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!mounted) return;
        setLoading(false);
        bridge.onLoadingChange(false);
      });
    return () => {
      mounted = false;
      bridge.onLoadingChange(false);
    };
  }, [bridge]);

  if (bridge?.kind !== "dry-run") return null;

  const groups = plan?.groups ?? [];
  const totalPages = Math.max(1, Math.ceil(groups.length / PAGE_SIZE));
  const currentPage = Math.min(page, totalPages);
  const visibleGroups = groups.slice(
    (currentPage - 1) * PAGE_SIZE,
    currentPage * PAGE_SIZE,
  );
  const totalDurationSeconds = groups.reduce(
    (sum, group) => sum + group.estimated_duration_ms / 1000,
    0,
  );
  const totalSizeBytes = groups.reduce(
    (sum, group) => sum + group.estimated_size_bytes,
    0,
  );
  const dstDir = (bridge.dstPath ?? "").replace(/\/+$/, "");

  return (
    <div className="shell-modal flex h-full flex-col overflow-hidden bg-surface-base text-fg-primary">
      <div className="border-border-subtle flex-1 overflow-auto border-b p-4">
        {loading ? (
          <div className="flex h-full min-h-64 flex-col items-center justify-center gap-3 text-fg-muted">
            <Spin />
            <span className="text-sm">正在分析...</span>
          </div>
        ) : error ? (
          <div className="border-border-subtle bg-surface-elevated rounded-md border p-4">
            <div className="text-fg-danger text-sm font-medium">
              模拟运行失败
            </div>
            <div className="text-fg-muted mt-2 whitespace-pre-wrap text-sm">
              {error}
            </div>
          </div>
        ) : groups.length === 0 ? (
          <p className="text-fg-muted text-sm">未找到可归并的视频文件。</p>
        ) : (
          <div className="space-y-3">
            {visibleGroups.map((group, pageIndex) => {
              const groupIndex = (currentPage - 1) * PAGE_SIZE + pageIndex;
              const fullOutput = dstDir
                ? `${dstDir}/${group.output_name}`
                : group.output_name;
              const isExpanded = expandedGroups.has(groupIndex);
              const timeRange = formatTimeRange(group.start_at, group.end_at);
              return (
                <div
                  key={`${group.output_name}:${group.input_files.join("|")}`}
                  className="border-border-subtle rounded-md border p-3"
                >
                  <div className="text-fg-muted mb-1 flex items-center justify-between gap-3 text-xs">
                    <span>
                      组 #{groupIndex + 1}
                      {timeRange ? ` · ${timeRange}` : ""}
                    </span>
                    <span className="shrink-0">
                      {group.encoder} ·{" "}
                      {formatDuration(
                        group.estimated_duration_ms / 1000,
                        formatLabels,
                      )}{" "}
                      · {formatBytes(group.estimated_size_bytes, formatLabels)}
                    </span>
                  </div>
                  <div className="text-fg-secondary space-y-0.5 font-mono text-xs">
                    <div className="flex items-start gap-2">
                      <span className="flex-1 break-all">
                        {group.input_files[0]}
                      </span>
                      {group.input_files.length > 1 && (
                        <button
                          type="button"
                          onClick={() => {
                            setExpandedGroups((prev) => {
                              const next = new Set(prev);
                              if (next.has(groupIndex)) {
                                next.delete(groupIndex);
                              } else {
                                next.add(groupIndex);
                              }
                              return next;
                            });
                          }}
                          className="text-fg-muted hover:text-fg-primary flex shrink-0 cursor-pointer items-center gap-0.5 rounded px-1 text-xs"
                        >
                          ({group.input_files.length}){isExpanded ? " ▲" : " ▼"}
                        </button>
                      )}
                    </div>
                    {isExpanded &&
                      group.input_files.slice(1).map((file) => (
                        <div key={file} className="break-all">
                          {file}
                        </div>
                      ))}
                  </div>
                  <div className="text-fg-primary mt-2 break-all font-mono text-xs">
                    {fullOutput}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 p-3 text-sm">
        <div className="text-fg-muted">
          {groups.length > 0
            ? `${groups.length} 组 · ${formatDuration(totalDurationSeconds, formatLabels)} · ${formatBytes(totalSizeBytes, formatLabels)}`
            : ""}
        </div>
        <div className="flex items-center gap-2">
          {groups.length > PAGE_SIZE && (
            <>
              <Button
                size="small"
                variant="secondary"
                disabled={currentPage <= 1}
                onClick={() => setPage((prev) => Math.max(1, prev - 1))}
              >
                上一页
              </Button>
              <span className="text-fg-muted text-xs">
                {currentPage}/{totalPages}
              </span>
              <Button
                size="small"
                variant="secondary"
                disabled={currentPage >= totalPages}
                onClick={() =>
                  setPage((prev) => Math.min(totalPages, prev + 1))
                }
              >
                下一页
              </Button>
            </>
          )}
          <Button size="small" onClick={() => win.close()}>
            关闭
          </Button>
        </div>
      </div>
    </div>
  );
}
