/**
 * Run panel: execute source and monitor progress via SSE.
 */
import { Button } from "@tokimo/ui";
import { useEffect, useState } from "react";
import type { ProgressEvent } from "./api";
import { cancelRun, runSource, subscribeRunProgress } from "./api";

interface Props {
  sourceId: string;
  t: (key: string) => string;
}

export function RunPanel({ sourceId, t }: Props) {
  const [running, setRunning] = useState(false);
  const [runId, setRunId] = useState<string | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);

  useEffect(() => {
    if (!runId) return;
    setRunning(true);
    const unsub = subscribeRunProgress(
      runId,
      (evt) => {
        setProgress(evt);
        // SSE doesn't send a "done" event explicitly, but we can infer from percent
        if (evt.percent >= 100) {
          setRunning(false);
        }
      },
      (err) => {
        console.error("SSE error:", err);
        setRunning(false);
      },
    );
    return () => {
      unsub();
    };
  }, [runId]);

  const handleRun = async () => {
    try {
      const res = await runSource(sourceId);
      setRunId(res.run_id);
      setProgress(null);
      alert(t("runStarted"));
    } catch (err) {
      alert(
        `${t("errorRun")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  };

  const handleCancel = async () => {
    if (!runId) return;
    try {
      await cancelRun(runId);
      setRunning(false);
      setRunId(null);
      setProgress(null);
      alert(t("runCancelled"));
    } catch (err) {
      alert(
        `${t("errorCancel")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  };

  return (
    <div className="space-y-4 p-4">
      <div className="flex gap-3">
        <Button
          onClick={handleRun}
          disabled={running}
          className="flex-1"
          size="large"
        >
          {t("btnRunNow")}
        </Button>
        {running && (
          <Button
            onClick={handleCancel}
            variant="danger"
            size="large"
            className="flex-1"
          >
            {t("btnCancel")}
          </Button>
        )}
      </div>

      {running && progress && (
        <div className="bg-surface-raised border-border-base space-y-3 rounded-md border p-4">
          <div>
            <div className="text-fg-secondary mb-1 text-xs">{t("phase")}</div>
            <div className="text-fg-primary text-sm font-medium">
              {progress.phase}
            </div>
          </div>

          <div>
            <div className="text-fg-secondary mb-2 flex items-center justify-between text-xs">
              <span>{t("progress")}</span>
              <span>{progress.percent.toFixed(1)}%</span>
            </div>
            <div className="bg-surface-base h-2 overflow-hidden rounded-full">
              <div
                className="bg-accent h-full transition-all duration-300"
                style={{ width: `${progress.percent}%` }}
              />
            </div>
          </div>

          {progress.current_file && (
            <div>
              <div className="text-fg-secondary mb-1 text-xs">
                {t("currentFile")}
              </div>
              <div className="text-fg-muted truncate text-xs font-mono">
                {progress.current_file}
              </div>
            </div>
          )}

          <div className="flex justify-between gap-4">
            <div>
              <div className="text-fg-secondary text-xs">{t("okCount")}</div>
              <div className="text-state-success-text text-lg font-semibold">
                {progress.ok_count}
              </div>
            </div>
            <div>
              <div className="text-fg-secondary text-xs">
                {t("failedCount")}
              </div>
              <div className="text-state-danger-text text-lg font-semibold">
                {progress.failed_count}
              </div>
            </div>
            <div>
              <div className="text-fg-secondary text-xs">
                {t("totalGroups")}
              </div>
              <div className="text-fg-primary text-lg font-semibold">
                {progress.group_count}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
