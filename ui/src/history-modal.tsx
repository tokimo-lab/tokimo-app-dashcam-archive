/**
 * History drawer: wraps HistoryPanel for a selected source.
 */
import { Drawer } from "@tokimo/ui";
import type { SourceDto } from "./api";
import { HistoryPanel } from "./history-panel";

interface Props {
  source: SourceDto | null;
  onClose: () => void;
  t: (key: string) => string;
}

export function HistoryModal({ source, onClose, t }: Props) {
  return (
    <Drawer
      open={source !== null}
      title={
        source
          ? `${source.name} — ${t("modalHistoryTitle")}`
          : t("modalHistoryTitle")
      }
      placement="right"
      width={680}
      onClose={onClose}
      maskClosable
      bodyStyle={{ padding: 0, overflow: "auto" }}
    >
      {source !== null && <HistoryPanel sourceId={source.id} t={t} />}
    </Drawer>
  );
}
