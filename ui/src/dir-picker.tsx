/**
 * DirPicker — choose a (sourceId, path) pair representing a directory in a
 * registered host VFS source. Re-implementation built on top of
 * `@tokimo/ui` PathSelector + Select, replacing the previous version that
 * depended on the host's now-removed `shell.pickStorageBinding` modal.
 *
 * Layout:
 *
 *   [ Source ▼ ]     ← Select listing all VFS sources
 *   [ /path  📁 ❌ ]  ← PathSelector w/ browse adapter + a clear button
 *
 * The legacy free-form `legacyPath` (pre-StorageBinding DB rows) is rendered
 * as a hint below the row when there is no current binding.
 */

import type { ShellApi } from "@tokimo/sdk";
import {
  Button,
  CloseOutlined,
  PathSelector,
  Select,
  type SelectOption,
} from "@tokimo/ui";
import { useEffect, useState } from "react";
import { listVfsSources, type VfsDto } from "./api";
import { useVfsBrowse } from "./hooks/useVfsBrowse";
import type { StorageBinding } from "./storage-binding";

function protocolPrefixFor(source: VfsDto | undefined): string | undefined {
  if (!source) return undefined;
  // SMB / NFS-style remote sources benefit from a protocol breadcrumb in
  // the file browser. Local sources don't need one.
  if (source.type === "local") return undefined;
  return `${source.type}://${source.name}`;
}

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
  const [sources, setSources] = useState<VfsDto[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listVfsSources()
      .then((list) => {
        if (cancelled) return;
        setSources(list);
        setLoadError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setLoadError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onBrowse = useVfsBrowse(shell);

  const selected = sources.find((s) => s.id === value?.sourceId);
  const sourceOptions: SelectOption[] = sources.map((s) => ({
    label: `${s.name} (${s.type})`,
    value: s.id,
    description: s.type,
  }));

  const updateSource = (sourceId: string | undefined) => {
    if (!sourceId) {
      onChange(null);
      return;
    }
    const src = sources.find((s) => s.id === sourceId);
    if (!src) return;
    onChange({
      sourceId: src.id,
      sourceType: src.type,
      sourceName: src.name,
      displayHints: undefined,
      path: value?.sourceId === src.id ? value.path : "",
    });
  };

  const updatePath = (path: string) => {
    if (!value) return;
    onChange({ ...value, path });
  };

  const protocolPrefix = protocolPrefixFor(selected);

  return (
    <div className="space-y-2">
      <div className="flex gap-2">
        <div className="min-w-0 flex-1">
          <Select
            value={value?.sourceId}
            onChange={(v) => updateSource(typeof v === "string" ? v : undefined)}
            options={sourceOptions}
            placeholder={t("storageBindingPlaceholder")}
            allowClear
            loading={loading}
            disabled={Boolean(loadError)}
          />
        </div>
        <Button
          icon={<CloseOutlined />}
          onClick={() => onChange(null)}
          aria-label={t("btnClear")}
          disabled={!value}
        />
      </div>

      <PathSelector
        value={value?.path ?? ""}
        onChange={updatePath}
        onBrowse={value ? onBrowse : undefined}
        sourceId={
          value && value.sourceType !== "local" ? value.sourceId : undefined
        }
        protocolPrefix={protocolPrefix}
        disabled={!value}
        browseLabel={t("btnBrowse")}
      />

      {loadError && (
        <p className="text-fg-danger text-xs">{loadError}</p>
      )}
      {!value && legacyPath && (
        <p className="text-fg-muted text-xs">
          <span className="bg-amber-500/10 text-amber-600 dark:text-amber-300 mr-1 inline-block rounded px-1.5 py-0.5 text-[10px] font-medium">
            {t("legacyStorageBinding")}
          </span>
          {legacyPath}
        </p>
      )}
    </div>
  );
}
