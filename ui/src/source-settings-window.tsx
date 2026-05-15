import { makeTranslator, type ShellWindowHandle } from "@tokimo/sdk";
import { useMemo, useState } from "react";
import { enUS, zhCN } from "./i18n";
import { getBridge } from "./modal-bridge";
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
  // Snapshot bridge entry at mount. Using useState init function so we
  // only read the registry once; subsequent re-renders (e.g. host shake
  // animation, prop changes) use the snapshot. NEVER clear the bridge
  // via useEffect cleanup — React 18 StrictMode dev double-invokes
  // mount effects (mount → cleanup → mount), which would wipe the
  // registry entry the instant the modal commits, leaving subsequent
  // re-renders to fall back to `return null` and the content disappears.
  const [bridge] = useState(() => (bridgeId ? getBridge(bridgeId) : undefined));

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
