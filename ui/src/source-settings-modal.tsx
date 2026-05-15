/**
 * Settings drawer wrapping SourceForm for editing/creating a source.
 */
import type { ShellApi } from "@tokimo/sdk";
import { Drawer } from "@tokimo/ui";
import type { SourceDto } from "./api";
import { SourceForm } from "./source-form";

interface Props {
  /** null = closed, "create" = new source, SourceDto = edit existing */
  source: SourceDto | "create" | null;
  onClose: () => void;
  onSaved: () => void;
  onDeleted: () => void;
  shell: ShellApi;
  t: (key: string) => string;
}

export function SourceSettingsModal({
  source,
  onClose,
  onSaved,
  onDeleted,
  shell,
  t,
}: Props) {
  const open = source !== null;
  const title =
    source === "create" ? t("modalNewSource") : t("modalEditSource");

  return (
    <Drawer
      open={open}
      title={title}
      placement="right"
      width={520}
      onClose={onClose}
      maskClosable
      bodyStyle={{
        padding: 0,
        overflow: "hidden",
        display: "flex",
        flexDirection: "column",
      }}
    >
      {source !== null && (
        <SourceForm
          source={source}
          onSaved={() => {
            onSaved();
            onClose();
          }}
          onDeleted={() => {
            onDeleted();
            onClose();
          }}
          shell={shell}
          t={t}
        />
      )}
    </Drawer>
  );
}
