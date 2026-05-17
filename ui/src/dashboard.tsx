/**
 * Main dashboard: library stats bar + responsive source card grid.
 */
import type { ShellApi } from "@tokimo/sdk";
import { Button, Empty, Spin } from "@tokimo/ui";
import { Plus } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { SourceDto } from "./api";
import { getSources } from "./api";
import { registerBridge } from "./modal-bridge";
import { SourceCard } from "./source-card";

interface Props {
  shell: ShellApi;
  t: (key: string) => string;
  locale: string;
}

export function Dashboard({ shell, t, locale }: Props) {
  const [sources, setSources] = useState<SourceDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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

  const handleSaved = useCallback(() => {
    loadSources();
  }, [loadSources]);

  const handleDeleted = useCallback(() => {
    loadSources();
  }, [loadSources]);

  const openSourceSettings = useCallback(
    (source: SourceDto | "create") => {
      const bridgeId = registerBridge({
        kind: "source-settings",
        shell,
        source,
        onSaved: handleSaved,
        onDeleted: handleDeleted,
      });
      shell.openModalWindow({
        component: () => import("./source-settings-window"),
        title: source === "create" ? t("modalNewSource") : t("modalEditSource"),
        width: 520,
        height: 720,
        metadata: { bridgeId, locale },
      });
    },
    [handleDeleted, handleSaved, locale, shell, t],
  );

  const openHistory = useCallback(
    (source: SourceDto) => {
      const bridgeId = registerBridge({
        kind: "history",
        sourceId: source.id,
        sourceName: source.name,
      });
      shell.openModalWindow({
        component: () => import("./history-window"),
        title: `${source.name} — ${t("modalHistoryTitle")}`,
        width: 680,
        height: 720,
        metadata: { bridgeId, locale },
      });
    },
    [locale, shell, t],
  );

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
            onClick={() => openSourceSettings("create")}
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
                onSettingsClick={() => openSourceSettings(source)}
                onToggle={(enabled) => {
                  setSources((prev) =>
                    prev.map((s) =>
                      s.id === source.id ? { ...s, enabled } : s,
                    ),
                  );
                }}
                onViewHistory={() => openHistory(source)}
                t={t}
                shell={shell}
                locale={locale}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
