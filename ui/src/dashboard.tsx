/**
 * Main dashboard: library stats bar + responsive source card grid.
 */
import type { ShellApi } from "@tokimo/sdk";
import { Button, Empty, Spin } from "@tokimo/ui";
import { Plus } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { SourceDto } from "./api";
import { getSources } from "./api";
import { HistoryModal } from "./history-modal";
import { SourceCard } from "./source-card";
import { SourceSettingsModal } from "./source-settings-modal";

interface Props {
  shell: ShellApi;
  t: (key: string) => string;
}

export function Dashboard({ shell, t }: Props) {
  const [sources, setSources] = useState<SourceDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Modal states
  const [settingsSource, setSettingsSource] = useState<
    SourceDto | "create" | null
  >(null);
  const [historySource, setHistorySource] = useState<SourceDto | null>(null);

  const loadSources = useCallback(() => {
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

  useEffect(() => loadSources(), [loadSources]);

  const handleSaved = () => {
    loadSources();
  };

  const handleDeleted = () => {
    loadSources();
  };

  const enabledCount = sources.filter((s) => s.enabled).length;

  return (
    <div className="flex h-full flex-col bg-surface-base text-fg-primary">
      {/* Library stats bar */}
      <div className="border-border-subtle shrink-0 border-b bg-surface-elevated px-6 py-3">
        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-6">
            <div>
              <span className="text-fg-primary text-2xl font-bold">
                {sources.length}
              </span>
              <span className="text-fg-secondary ml-1 text-sm">
                {t("totalSources")}
              </span>
            </div>
            <div className="border-border-subtle border-l pl-6">
              <span className="text-fg-primary text-2xl font-bold">
                {enabledCount}
              </span>
              <span className="text-fg-secondary ml-1 text-sm">
                {t("enabledSources")}
              </span>
            </div>
          </div>
          <Button
            onClick={() => setSettingsSource("create")}
            className="flex items-center gap-1"
          >
            <Plus size={16} />
            {t("addSource")}
          </Button>
        </div>
      </div>

      {/* Main content */}
      <div className="flex-1 overflow-y-auto p-6">
        {loading ? (
          <div className="flex h-full items-center justify-center">
            <Spin />
          </div>
        ) : error ? (
          <div className="flex h-full flex-col items-center justify-center gap-2">
            <p className="text-fg-danger">{t("errorLoad")}</p>
            <p className="text-fg-muted text-sm">{error}</p>
          </div>
        ) : sources.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <Empty
              description={
                <div className="text-center">
                  <p className="text-fg-secondary mb-1">{t("noSources")}</p>
                  <p className="text-fg-muted text-sm">{t("noSourcesAdd")}</p>
                </div>
              }
            />
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {sources.map((source) => (
              <SourceCard
                key={source.id}
                source={source}
                onSettingsClick={() => setSettingsSource(source)}
                onToggle={(enabled) => {
                  setSources((prev) =>
                    prev.map((s) =>
                      s.id === source.id ? { ...s, enabled } : s,
                    ),
                  );
                }}
                onViewHistory={() => setHistorySource(source)}
                t={t}
              />
            ))}
          </div>
        )}
      </div>

      {/* Settings drawer */}
      <SourceSettingsModal
        source={settingsSource}
        onClose={() => setSettingsSource(null)}
        onSaved={handleSaved}
        onDeleted={handleDeleted}
        shell={shell}
        t={t}
      />

      {/* History drawer */}
      <HistoryModal
        source={historySource}
        onClose={() => setHistorySource(null)}
        t={t}
      />
    </div>
  );
}
