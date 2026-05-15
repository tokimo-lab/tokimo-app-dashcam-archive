import { makeTranslator, type ShellWindowHandle } from "@tokimo/sdk";
import { useMemo, useState } from "react";
import { HistoryPanel } from "./history-panel";
import { enUS, zhCN } from "./i18n";
import { getBridge } from "./modal-bridge";

export default function HistoryWindow({ win }: { win: ShellWindowHandle }) {
  const bridgeId =
    typeof win.metadata.bridgeId === "string" ? win.metadata.bridgeId : "";
  const locale =
    typeof win.metadata.locale === "string" ? win.metadata.locale : "zh-CN";
  const t = useMemo(
    () => makeTranslator({ "zh-CN": zhCN, "en-US": enUS }, locale),
    [locale],
  );
  // Snapshot bridge at mount; never re-read from registry on re-render.
  // See source-settings-window.tsx for why useEffect cleanup is unsafe.
  const [bridge] = useState(() => (bridgeId ? getBridge(bridgeId) : undefined));

  if (bridge?.kind !== "history") return null;

  return (
    <div className="shell-modal h-full overflow-auto bg-surface-base text-fg-primary">
      <HistoryPanel sourceId={bridge.sourceId} t={t} />
    </div>
  );
}
