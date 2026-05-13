import type { ShellApi } from "@tokimo/sdk";
import { Button, FolderOpenOutlined, Input } from "@tokimo/ui";

export interface DirPickerProps {
  value: string;
  onChange: (path: string) => void;
  shell: ShellApi;
  t: (key: string) => string;
}

export function DirPicker({ value, onChange, shell, t }: DirPickerProps) {
  const handleBrowse = async () => {
    const result = await shell.pickFilePath({
      initialPath: value || undefined,
      title: t("dirPickerTitle"),
    });
    if (result !== null) onChange(result);
  };

  return (
    <div className="flex gap-2">
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="flex-1"
      />
      <Button
        icon={<FolderOpenOutlined />}
        onClick={handleBrowse}
        aria-label={t("btnBrowse")}
      />
    </div>
  );
}
