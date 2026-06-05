/**
 * History panel: list runs and expand to show groups + warnings.
 */
import { Empty, Spin, Tag } from "@tokimo/ui";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useEffect, useState } from "react";
import type { FormatLabels, GroupDto, RunDetailDto, RunDto } from "./api";
import {
  formatBytes,
  formatDuration,
  getRunDetail,
  getSourceRuns,
} from "./api";

interface Props {
  sourceId: string;
  t: (key: string) => string;
}

export function HistoryPanel({ sourceId, t }: Props) {
  const [runs, setRuns] = useState<RunDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [details, setDetails] = useState<Map<string, RunDetailDto>>(new Map());

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    getSourceRuns(sourceId, 50)
      .then((data) => {
        if (mounted) {
          setRuns(data);
          setLoading(false);
        }
      })
      .catch((err) => {
        console.error("Load runs failed:", err);
        if (mounted) setLoading(false);
      });
    return () => {
      mounted = false;
    };
  }, [sourceId]);

  const toggleExpand = async (runId: string) => {
    const next = new Set(expanded);
    if (next.has(runId)) {
      next.delete(runId);
    } else {
      next.add(runId);
      if (!details.has(runId)) {
        try {
          const detail = await getRunDetail(runId);
          setDetails(new Map(details).set(runId, detail));
        } catch (err) {
          console.error("Load run detail failed:", err);
        }
      }
    }
    setExpanded(next);
  };

  const statusColor = (status: RunDto["status"]) => {
    switch (status) {
      case "succeeded":
        return "success";
      case "failed":
        return "error";
      case "running":
        return "processing";
      case "cancelled":
        return "default";
      case "queued":
        return "warning";
      default:
        return "default";
    }
  };

  const warningColor = (level: GroupDto["warning_level"]) => {
    switch (level) {
      case "clean":
        return "success";
      case "warn":
        return "warning";
      case "suspicious":
        return "warning";
      case "fatal":
        return "error";
      default:
        return "default";
    }
  };

  const formatLabels: FormatLabels = {
    empty: t("emptyValue"),
    byteUnits: [
      t("unitByte"),
      t("unitKilobyte"),
      t("unitMegabyte"),
      t("unitGigabyte"),
      t("unitTerabyte"),
    ],
    unitValueSeparator: t("separatorUnitValue"),
    secondUnit: t("unitSecondShort"),
    minuteUnit: t("unitMinuteShort"),
    hourUnit: t("unitHourShort"),
    durationPartsSeparator: t("separatorDurationParts"),
  };
  const formatByteValue = (bytes: number | null | undefined) =>
    formatBytes(bytes, formatLabels);
  const formatDurationValue = (seconds: number | null | undefined) =>
    formatDuration(seconds, formatLabels);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spin />
      </div>
    );
  }

  if (runs.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-4">
        <Empty description={t("noHistory")} />
      </div>
    );
  }

  return (
    <div className="space-y-2 p-4">
      {runs.map((run) => {
        const isExpanded = expanded.has(run.id);
        const detail = details.get(run.id);
        return (
          <div
            key={run.id}
            className="bg-surface-raised border-border-base rounded-md border"
          >
            <button
              type="button"
              onClick={() => toggleExpand(run.id)}
              className="w-full cursor-pointer p-3 text-left hover:bg-surface-overlay"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="flex min-w-0 flex-1 items-center gap-2">
                  {isExpanded ? (
                    <ChevronDown
                      size={16}
                      className="text-fg-secondary shrink-0"
                    />
                  ) : (
                    <ChevronRight
                      size={16}
                      className="text-fg-secondary shrink-0"
                    />
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="text-fg-primary mb-1 text-sm">
                      {new Date(run.started_at).toLocaleString()}
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <Tag color={statusColor(run.status)}>
                        {t(
                          `status${run.status.charAt(0).toUpperCase() + run.status.slice(1)}`,
                        )}
                      </Tag>
                      <Tag color="default">
                        {t(
                          `trigger${run.trigger.charAt(0).toUpperCase() + run.trigger.slice(1)}`,
                        )}
                      </Tag>
                      <span className="text-fg-secondary text-xs">
                        {run.total_groups} {t("totalGroups")}{" "}
                        {t("separatorMeta")} {run.ok_groups} {t("okGroups")}{" "}
                        {t("separatorMeta")} {run.failed_groups}{" "}
                        {t("failedGroupsLabel")}
                      </span>
                    </div>
                  </div>
                </div>
                <div className="text-fg-muted shrink-0 text-right text-xs">
                  {run.bytes_in != null && run.bytes_out != null && (
                    <div>
                      {formatByteValue(run.bytes_in)} {t("separatorFlow")}{" "}
                      {formatByteValue(run.bytes_out)}
                    </div>
                  )}
                  {run.finished_at && (
                    <div>
                      {formatDurationValue(
                        Math.floor(
                          (new Date(run.finished_at).getTime() -
                            new Date(run.started_at).getTime()) /
                            1000,
                        ),
                      )}
                    </div>
                  )}
                </div>
              </div>
            </button>

            {isExpanded && detail && (
              <div className="border-border-subtle border-t p-3">
                {detail.groups.length === 0 ? (
                  <div className="text-fg-muted text-center text-sm">
                    {t("noGroups")}
                  </div>
                ) : (
                  <div className="space-y-4">
                    {groupByCameraKey(detail.groups).map(
                      ([cameraKey, groups]) => (
                        <div key={cameraKey}>
                          <div className="text-fg-primary mb-2 text-xs font-semibold">
                            {t("groupCamera")}
                            {t("separatorLabelValue")}
                            {cameraKey}
                          </div>
                          <div className="space-y-2">
                            {groups.map((grp) => {
                              const grpWarnings = detail.warnings.filter(
                                (w) => w.group_id === grp.id,
                              );
                              return (
                                <div
                                  key={grp.id}
                                  className="bg-surface-base rounded border border-border-subtle p-2 text-xs"
                                >
                                  <div className="mb-1 flex items-center gap-2">
                                    <Tag
                                      color={warningColor(grp.warning_level)}
                                    >
                                      {t(
                                        `warningLevel${grp.warning_level.charAt(0).toUpperCase() + grp.warning_level.slice(1)}`,
                                      )}
                                    </Tag>
                                    <span className="text-fg-secondary">
                                      {grp.start_dt
                                        ? new Date(
                                            grp.start_dt,
                                          ).toLocaleString()
                                        : t("emptyValue")}{" "}
                                      {t("separatorTimeRange")}{" "}
                                      {grp.end_dt
                                        ? new Date(grp.end_dt).toLocaleString()
                                        : t("emptyValue")}
                                    </span>
                                  </div>
                                  <div className="text-fg-muted mb-1">
                                    {t("groupOutput")}
                                    {t("separatorLabelValue")}
                                    {grp.output_path}
                                  </div>
                                  <div className="text-fg-muted">
                                    {t("groupDecision")}
                                    {t("separatorLabelValue")}
                                    {grp.decision} {t("separatorMeta")}{" "}
                                    {t("groupStatus")}
                                    {t("separatorLabelValue")}
                                    {grp.status}
                                    {grp.duration_secs != null && (
                                      <>
                                        {" "}
                                        {t("separatorMeta")}{" "}
                                        {formatDurationValue(grp.duration_secs)}
                                      </>
                                    )}
                                    {grp.bytes_in != null &&
                                      grp.bytes_out != null && (
                                        <>
                                          {" "}
                                          {t("separatorMeta")}{" "}
                                          {formatByteValue(grp.bytes_in)}{" "}
                                          {t("separatorFlow")}{" "}
                                          {formatByteValue(grp.bytes_out)}
                                        </>
                                      )}
                                  </div>
                                  {grp.abort_reason && (
                                    <div className="text-state-danger-text mt-1">
                                      {t("abortLabel")}
                                      {t("separatorLabelValue")}
                                      {grp.abort_reason}
                                    </div>
                                  )}
                                  {grpWarnings.length > 0 && (
                                    <div className="mt-2 space-y-1">
                                      <div className="text-fg-secondary font-semibold">
                                        {t("warnings")}
                                        {t("separatorLabel")}
                                      </div>
                                      {grpWarnings.map((w) => (
                                        <div
                                          key={w.id}
                                          className="text-fg-muted"
                                        >
                                          {w.warning_key}
                                          {t("separatorWarningCountPrefix")}
                                          {w.count}
                                          {t("separatorWarningCountSuffix")}
                                          {w.first_example && (
                                            <>
                                              {t("separatorWarningExample")}
                                              {w.first_example}
                                            </>
                                          )}
                                        </div>
                                      ))}
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        </div>
                      ),
                    )}
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function groupByCameraKey(groups: GroupDto[]): [string, GroupDto[]][] {
  const map = new Map<string, GroupDto[]>();
  for (const g of groups) {
    const key = g.camera_key;
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(g);
  }
  return Array.from(map.entries());
}
