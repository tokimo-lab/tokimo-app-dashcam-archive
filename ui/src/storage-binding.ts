import type { ShellApi } from "@tokimo/sdk";

// Mirrors @tokimo/sdk StorageBinding; local package typings can lag host SDK.
export interface StorageBinding {
  sourceId: string;
  sourceType: string;
  sourceName: string;
  displayHints?: { protocolPrefix?: string; rootPath?: string };
  path: string;
}

interface StorageBindingPickerParams {
  initial?: { sourceId?: string; path?: string };
  title?: string;
}

interface StorageBindingShell {
  pickStorageBinding: (
    params?: StorageBindingPickerParams,
  ) => Promise<StorageBinding | null>;
}

export function hasStorageBindingPicker(
  shell: ShellApi,
): shell is ShellApi & StorageBindingShell {
  const candidate = shell as ShellApi & { pickStorageBinding?: unknown };
  return typeof candidate.pickStorageBinding === "function";
}
