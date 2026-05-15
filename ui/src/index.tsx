/**
 * Dashcam Archive standalone app entry point.
 */
import type { AppRuntimeCtx, Dispose } from "@tokimo/sdk";
import { defineApp, makeTranslator } from "@tokimo/sdk";
import {
  ConfigProvider,
  ToastProvider,
  enUS as uiEnUS,
  zhCN as uiZhCN,
} from "@tokimo/ui";
import { StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { Dashboard } from "./dashboard";
import { enUS, zhCN } from "./i18n";
import "./index.css";

function DashcamArchiveApp({ ctx }: { ctx: AppRuntimeCtx }) {
  const t = makeTranslator({ "zh-CN": zhCN, "en-US": enUS }, ctx.locale);
  const locale = ctx.locale === "zh-CN" ? uiZhCN : uiEnUS;

  return (
    <ConfigProvider locale={locale}>
      <ToastProvider>
        <Dashboard shell={ctx.shell} t={t} />
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
