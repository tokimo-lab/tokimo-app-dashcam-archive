/**
 * Directory picker with file browser modal.
 */
import { Button, Input, Modal } from "@tokimo/ui";
import { useState } from "react";

export interface DirPickerProps {
  value: string;
  onChange: (path: string) => void;
  t: (key: string) => string;
}

interface BrowseEntry {
  name: string;
  path: string;
  isDirectory: boolean;
  size: number | null;
  modifiedAt: string;
}

interface BrowseResponse {
  success: true;
  data: {
    currentPath: string;
    parentPath: string | null;
    entries: BrowseEntry[];
  };
}

export function DirPicker({ value, onChange, t }: DirPickerProps) {
  const [open, setOpen] = useState(false);
  const [browsePath, setBrowsePath] = useState(value || "/");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [browseData, setBrowseData] = useState<BrowseResponse["data"] | null>(
    null,
  );

  const fetchBrowse = async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(
        `/api/vfs/local/browse?path=${encodeURIComponent(path)}`,
        { credentials: "include" },
      );
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`HTTP ${res.status}: ${text}`);
      }
      const data = (await res.json()) as BrowseResponse;
      setBrowseData(data.data);
      setBrowsePath(data.data.currentPath);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBrowseData(null);
    } finally {
      setLoading(false);
    }
  };

  const navigate = (path: string) => {
    fetchBrowse(path);
  };

  const handleOpen = () => {
    const startPath = value || "/";
    setBrowsePath(startPath);
    setOpen(true);
    fetchBrowse(startPath);
  };

  const handleOk = () => {
    onChange(browseData?.currentPath ?? browsePath);
    setOpen(false);
  };

  const handleCancel = () => {
    setOpen(false);
  };

  const directories = browseData?.entries.filter((e) => e.isDirectory) ?? [];

  return (
    <>
      <div className="flex gap-2">
        <Input
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="flex-1"
        />
        <Button onClick={handleOpen}>{t("btnBrowse")}</Button>
      </div>

      <Modal
        open={open}
        title={t("dirPickerTitle")}
        okText={t("confirmDirPick")}
        cancelText={t("btnCancel")}
        onOk={handleOk}
        onCancel={handleCancel}
      >
        <div className="text-fg-muted mb-2 font-mono text-xs">
          {browseData?.currentPath ?? browsePath}
        </div>

        {loading && (
          <div className="text-fg-secondary text-sm">{t("loading")}</div>
        )}

        {error && <div className="text-fg-danger text-sm">{error}</div>}

        {!loading && !error && browseData && (
          <div className="max-h-96 space-y-1 overflow-y-auto">
            {browseData.parentPath !== null && (
              <button
                type="button"
                onClick={() => navigate(browseData.parentPath ?? "/")}
                className="text-fg-secondary w-full text-left cursor-pointer px-3 py-2 rounded hover:bg-surface-elevated text-sm"
              >
                .. {t("parentDir")}
              </button>
            )}
            {directories.map((entry) => (
              <button
                key={entry.path}
                type="button"
                onClick={() => navigate(entry.path)}
                className="text-fg-primary w-full text-left cursor-pointer px-3 py-2 rounded hover:bg-surface-elevated text-sm"
              >
                {entry.name}/
              </button>
            ))}
            {directories.length === 0 && browseData.parentPath === null && (
              <div className="text-fg-muted px-3 py-2 text-sm">
                {t("noDirectories")}
              </div>
            )}
          </div>
        )}
      </Modal>
    </>
  );
}
