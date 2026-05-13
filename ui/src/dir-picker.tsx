import type { ShellApi } from "@tokimo/sdk";
import { Button, CloseOutlined, FolderOpenOutlined } from "@tokimo/ui";
import type { StorageBinding } from "./storage-binding";
import { hasStorageBindingPicker } from "./storage-binding";

export interface DirPickerProps {
  value: StorageBinding | null;
  onChange: (binding: StorageBinding | null) => void;
  shell: ShellApi;
  t: (key: string) => string;
  legacyPath?: string | null;
}

export function DirPicker({
  value,
  onChange,
  shell,
  t,
  legacyPath,
}: DirPickerProps) {
  const handleBrowse = async () => {
    if (!hasStorageBindingPicker(shell)) {
      throw new Error("Storage binding picker is not available");
    }
    const result = await shell.pickStorageBinding({
      initial: value
        ? { sourceId: value.sourceId, path: value.path }
        : undefined,
      title: t("storageBindingPickerTitle"),
    });
    if (result !== null) onChange(result);
  };

  const hasLegacyPath = !value && Boolean(legacyPath);

  return (
    <div className="flex gap-2">
      <button
        type="button"
        onClick={handleBrowse}
        className="border-border-base bg-surface-elevated hover:bg-surface-glass min-w-0 flex-1 cursor-pointer rounded-md border px-3 py-1.5 text-left text-sm transition-colors"
      >
        {value ? (
          <span className="flex min-w-0 items-center gap-2">
            {value.sourceType !== "local" && (
              <span className="bg-blue-50 text-blue-600 dark:bg-sky-500/[0.12] dark:text-sky-300 shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium uppercase">
                {value.sourceType || t("remoteStorageSource")}
              </span>
            )}
            <span className="min-w-0 truncate">
              {value.sourceName} :: {value.path}
            </span>
          </span>
        ) : hasLegacyPath ? (
          <span className="flex min-w-0 items-center gap-2">
            <span className="bg-amber-500/10 text-amber-600 dark:text-amber-300 shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium">
              {t("legacyStorageBinding")}
            </span>
            <span className="min-w-0 truncate">{legacyPath}</span>
          </span>
        ) : (
          <span className="text-fg-muted">{t("storageBindingPlaceholder")}</span>
        )}
      </button>
      <Button
        icon={<FolderOpenOutlined />}
        onClick={handleBrowse}
        aria-label={t("btnBrowse")}
      />
      <Button
        icon={<CloseOutlined />}
        onClick={() => onChange(null)}
        aria-label={t("btnClear")}
        disabled={!value && !hasLegacyPath}
      />
    </div>
  );
}
