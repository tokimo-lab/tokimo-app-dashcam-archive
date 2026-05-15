import type { ShellApi } from "@tokimo/sdk";
import type { SourceDto } from "./api";

interface SourceSettingsBridge {
  kind: "source-settings";
  shell: ShellApi;
  source: SourceDto | "create";
  onSaved: () => void;
  onDeleted: () => void;
}

interface HistoryBridge {
  kind: "history";
  sourceId: string;
  sourceName: string;
}

export type ModalBridge = SourceSettingsBridge | HistoryBridge;

const registry = new Map<string, ModalBridge>();
let counter = 0;

export function registerBridge(b: ModalBridge): string {
  counter += 1;
  const id = `dashcam-bridge-${Date.now()}-${counter}`;
  registry.set(id, b);
  return id;
}

export function getBridge(id: string): ModalBridge | undefined {
  return registry.get(id);
}

/**
 * ⚠️ Do NOT call clearBridge from useEffect cleanup in modal windows.
 * React 18 StrictMode dev double-invokes mount effects (mount → cleanup
 * → mount), which would wipe the entry instantly after the modal commits.
 * Subsequent re-renders (e.g. host shake animation) would then fall back
 * to `return null` and the modal content would disappear.
 *
 * Modal windows must snapshot the bridge once via `useState(() => getBridge(id))`.
 * Letting entries accumulate is fine — bounded by # of modal opens per session.
 */
export function clearBridge(id: string): void {
  registry.delete(id);
}
