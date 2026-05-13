/**
 * Dashcam Archive standalone app entry point.
 */
import type { AppRuntimeCtx, Dispose } from "@tokimo/sdk";
import { defineApp, makeTranslator } from "@tokimo/sdk";
import {
  ConfigProvider,
  Empty,
  ToastProvider,
  enUS as uiEnUS,
  zhCN as uiZhCN,
} from "@tokimo/ui";
import { StrictMode, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import type { SourceDto } from "./api";
import { getSource } from "./api";
import { HistoryPanel } from "./history-panel";
import { enUS, zhCN } from "./i18n";
import "./index.css";
import { RunPanel } from "./run-panel";
import { SourceForm } from "./source-form";
import { SourcesList } from "./sources-list";

type Tab = "settings" | "run" | "history";

function DashcamArchiveApp({ ctx }: { ctx: AppRuntimeCtx }) {
  const t = makeTranslator({ "zh-CN": zhCN, "en-US": enUS }, ctx.locale);
  const [selected, setSelected] = useState<string | "create" | null>(null);
  const [selectedData, setSelectedData] = useState<SourceDto | "create" | null>(
    null,
  );
  const [tab, setTab] = useState<Tab>("settings");
  const [refresh, setRefresh] = useState(0);

  const handleSelect = async (id: string | "create") => {
    setSelected(id);
    setTab("settings");
    if (id === "create") {
      setSelectedData("create");
    } else {
      try {
        const data = await getSource(id);
        setSelectedData(data);
      } catch (err) {
        console.error("Load source failed:", err);
        setSelectedData(null);
      }
    }
  };

  const handleCreate = () => {
    setSelected("create");
    setSelectedData("create");
    setTab("settings");
  };

  const handleSaved = () => {
    setRefresh((prev) => prev + 1);
    setSelected(null);
    setSelectedData(null);
  };

  const handleDeleted = () => {
    setRefresh((prev) => prev + 1);
    setSelected(null);
    setSelectedData(null);
  };

  const locale = ctx.locale === "zh-CN" ? uiZhCN : uiEnUS;

  return (
    <ConfigProvider locale={locale}>
      <ToastProvider>
        <div className="flex h-full bg-surface-base text-fg-primary">
          {/* Left sidebar */}
          <div className="border-border-base w-80 shrink-0 border-r">
            <SourcesList
              selected={selected}
              onSelect={handleSelect}
              onCreate={handleCreate}
              refresh={refresh}
              t={t}
            />
          </div>

          {/* Right detail */}
          <div className="flex flex-1 flex-col overflow-hidden">
            {!selectedData ? (
              <div className="flex h-full items-center justify-center">
                <Empty
                  description={
                    <div className="text-center">
                      <p className="text-fg-secondary mb-1">
                        {t("selectSource")}
                      </p>
                      <p className="text-fg-muted text-sm">
                        {t("selectSourceDesc")}
                      </p>
                    </div>
                  }
                />
              </div>
            ) : (
              <>
                {/* Tabs */}
                <div className="border-border-subtle flex border-b">
                  <button
                    type="button"
                    onClick={() => setTab("settings")}
                    className={`cursor-pointer border-b-2 px-4 py-2 text-sm transition-colors ${
                      tab === "settings"
                        ? "border-accent text-accent"
                        : "border-transparent text-fg-secondary hover:text-fg-primary"
                    }`}
                  >
                    {t("tabSettings")}
                  </button>
                  {selectedData !== "create" && (
                    <>
                      <button
                        type="button"
                        onClick={() => setTab("run")}
                        className={`cursor-pointer border-b-2 px-4 py-2 text-sm transition-colors ${
                          tab === "run"
                            ? "border-accent text-accent"
                            : "border-transparent text-fg-secondary hover:text-fg-primary"
                        }`}
                      >
                        {t("tabRun")}
                      </button>
                      <button
                        type="button"
                        onClick={() => setTab("history")}
                        className={`cursor-pointer border-b-2 px-4 py-2 text-sm transition-colors ${
                          tab === "history"
                            ? "border-accent text-accent"
                            : "border-transparent text-fg-secondary hover:text-fg-primary"
                        }`}
                      >
                        {t("tabHistory")}
                      </button>
                    </>
                  )}
                </div>

                {/* Tab content */}
                <div className="flex-1 overflow-y-auto">
                  {tab === "settings" && (
                    <SourceForm
                      source={selectedData}
                      onSaved={handleSaved}
                      onDeleted={handleDeleted}
                      shell={ctx.shell}
                      t={t}
                    />
                  )}
                  {tab === "run" && selectedData !== "create" && (
                    <RunPanel sourceId={selectedData.id} t={t} />
                  )}
                  {tab === "history" && selectedData !== "create" && (
                    <HistoryPanel sourceId={selectedData.id} t={t} />
                  )}
                </div>
              </>
            )}
          </div>
        </div>
      </ToastProvider>
    </ConfigProvider>
  );
}

export default defineApp({
  id: "dashcam-archive",
  manifest: {
    id: "dashcam-archive",
    appName: "录像归并",
    icon: "Video",
    image: "icon.png",
    color: "#3b82f6",
    windowType: "dashcam-archive",
    defaultSize: { width: 1200, height: 800 },
    category: "app",
  },
  translations: { "zh-CN": zhCN, "en-US": enUS },
  mount(container: HTMLElement, ctx: AppRuntimeCtx): Dispose {
    const root: Root = createRoot(container);
    root.render(
      <StrictMode>
        <DashcamArchiveApp ctx={ctx} />
      </StrictMode>,
    );
    return () => root.unmount();
  },
});
