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

export function clearBridge(id: string): void {
  registry.delete(id);
}
