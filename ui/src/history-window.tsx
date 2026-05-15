import { makeTranslator, type ShellWindowHandle } from "@tokimo/sdk";
import { useEffect, useMemo } from "react";
import { HistoryPanel } from "./history-panel";
import { enUS, zhCN } from "./i18n";
import { clearBridge, getBridge } from "./modal-bridge";

export default function HistoryWindow({ win }: { win: ShellWindowHandle }) {
  const bridgeId =
    typeof win.metadata.bridgeId === "string" ? win.metadata.bridgeId : "";
  const locale =
    typeof win.metadata.locale === "string" ? win.metadata.locale : "zh-CN";
  const t = useMemo(
    () => makeTranslator({ "zh-CN": zhCN, "en-US": enUS }, locale),
    [locale],
  );
  const bridge = bridgeId ? getBridge(bridgeId) : undefined;

  useEffect(() => {
    return () => {
      if (bridgeId) clearBridge(bridgeId);
    };
  }, [bridgeId]);

  if (bridge?.kind !== "history") return null;

  return (
    <div className="shell-modal h-full overflow-auto bg-surface-base text-fg-primary">
      <HistoryPanel sourceId={bridge.sourceId} t={t} />
    </div>
  );
}
