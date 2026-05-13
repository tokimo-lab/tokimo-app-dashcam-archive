/**
 * Left sidebar: sources list with add button.
 */
import { Button, Empty, Spin, Switch, Tag } from "@tokimo/ui";
import { ChevronRight } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { SourceDto } from "./api";
import { getSources, updateSource } from "./api";

interface Props {
  selected: string | "create" | null;
  onSelect: (id: string | "create") => void;
  onCreate: () => void;
  refresh: number;
  t: (key: string) => string;
}

interface SourceLocationDisplay {
  text: string;
  legacy: boolean;
  remoteType: string | null;
}

function sourceLocationDisplay(
  source: SourceDto,
  prefix: "src" | "dst",
  t: (key: string) => string,
): SourceLocationDisplay {
  const path = prefix === "src" ? source.src_path : source.dst_path;
  const sourceId =
    prefix === "src" ? source.src_source_id : source.dst_source_id;
  const sourceType =
    prefix === "src" ? source.src_source_type : source.dst_source_type;
  const sourceName =
    prefix === "src" ? source.src_source_name : source.dst_source_name;

  if (!path) return { text: t("emptyValue"), legacy: false, remoteType: null };
  if (!sourceId || !sourceType) {
    return { text: path, legacy: true, remoteType: null };
  }

  return {
    text: `${sourceName || sourceType} :: ${path}`,
    legacy: false,
    remoteType: sourceType !== "local" ? sourceType : null,
  };
}

export function SourcesList({
  selected,
  onSelect,
  onCreate,
  refresh,
  t,
}: Props) {
  const [sources, setSources] = useState<SourceDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadSources = useCallback((refreshKey: number) => {
    void refreshKey;
    let mounted = true;
    setLoading(true);
    setError(null);
    getSources()
      .then((data) => {
        if (mounted) {
          setSources(data);
          setLoading(false);
        }
      })
      .catch((err) => {
        if (mounted) {
          setError(err instanceof Error ? err.message : String(err));
          setLoading(false);
        }
      });
    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => loadSources(refresh), [loadSources, refresh]);

  const handleToggle = async (id: string, enabled: boolean) => {
    try {
      await updateSource(id, { enabled });
      setSources((prev) =>
        prev.map((s) => (s.id === id ? { ...s, enabled } : s)),
      );
    } catch (err) {
      console.error("Toggle failed:", err);
    }
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spin />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-4">
        <p className="text-fg-danger">{t("errorLoad")}</p>
        <p className="text-fg-muted text-sm">{error}</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col bg-surface-base">
      <div className="border-border-subtle border-b p-3">
        <Button onClick={onCreate} className="w-full">
          {t("newSource")}
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {sources.length === 0 ? (
          <div className="flex h-full items-center justify-center p-4">
            <Empty description={t("noSourcesDesc")} />
          </div>
        ) : (
          <div className="space-y-1 p-2">
            {sources.map((source) => {
              const src = sourceLocationDisplay(source, "src", t);
              const dst = sourceLocationDisplay(source, "dst", t);
              return (
                <div
                  key={source.id}
                  className={`
                    rounded-md border transition-colors
                    ${
                      selected === source.id
                        ? "bg-accent border-accent text-fg-on-accent"
                        : "bg-surface-elevated border-border-base hover:bg-surface-glass"
                    }
                  `}
                >
                  <div className="flex items-start justify-between gap-2 p-3">
                    <button
                      type="button"
                      onClick={() => onSelect(source.id)}
                      className="min-w-0 flex-1 cursor-pointer text-left"
                    >
                      <div className="text-fg-primary mb-1 flex items-center gap-2 text-sm font-medium">
                        <span className="truncate">{source.name}</span>
                        {selected === source.id && <ChevronRight size={14} />}
                      </div>
                      <div className="text-fg-secondary mb-2 text-xs">
                        <span className="truncate block">
                          {src.text} {t("separatorFlow")} {dst.text}
                        </span>
                      </div>
                      <div className="flex flex-wrap items-center gap-2">
                        {(src.legacy || dst.legacy) && (
                          <Tag color="warning" size="small">
                            {t("legacyStorageBinding")}
                          </Tag>
                        )}
                        {src.remoteType && (
                          <Tag color="blue" size="small">
                            {src.remoteType}
                          </Tag>
                        )}
                        {dst.remoteType &&
                          dst.remoteType !== src.remoteType && (
                            <Tag color="blue" size="small">
                              {dst.remoteType}
                            </Tag>
                          )}
                        <Tag
                          color={
                            source.trigger_mode === "manual_only"
                              ? "default"
                              : "success"
                          }
                        >
                          {t(
                            source.trigger_mode === "manual_only"
                              ? "triggerManualOnly"
                              : source.trigger_mode === "cron"
                                ? "triggerCron"
                                : source.trigger_mode === "watcher"
                                  ? "triggerWatcher"
                                  : "triggerCronWatcher",
                          )}
                        </Tag>
                      </div>
                    </button>
                    <Switch
                      checked={source.enabled}
                      onChange={(checked) => handleToggle(source.id, checked)}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
