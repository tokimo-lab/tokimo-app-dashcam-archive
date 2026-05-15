import { makeTranslator, type ShellWindowHandle } from "@tokimo/sdk";
import { useEffect, useMemo } from "react";
import { enUS, zhCN } from "./i18n";
import { clearBridge, getBridge } from "./modal-bridge";
import { SourceForm } from "./source-form";

export default function SourceSettingsWindow({
  win,
}: {
  win: ShellWindowHandle;
}) {
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

  if (bridge?.kind !== "source-settings") return null;

  return (
    <div className="shell-modal flex h-full flex-col overflow-hidden bg-surface-base text-fg-primary">
      <SourceForm
        source={bridge.source}
        onSaved={() => {
          win.close();
          bridge.onSaved();
        }}
        onDeleted={() => {
          win.close();
          bridge.onDeleted();
        }}
        shell={bridge.shell}
        t={t}
      />
    </div>
  );
}
