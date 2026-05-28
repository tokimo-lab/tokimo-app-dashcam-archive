/**
 * VfsBrowserWindow — sidecar wrapper around @tokimo/ui FileBrowserWindow.
 *
 * Modal window opened via `useVfsBrowse`. Wraps the generic UI component
 * with the sidecar's vfs API client + bridge-based result delivery.
 */

import type { ShellWindowHandle } from "@tokimo/sdk";
import {
  ConfigProvider,
  type FileBrowserVfsApi,
  FileBrowserWindow,
  zhCN,
} from "@tokimo/ui";
import { useState } from "react";
import { browseVfs, statVfs } from "../api";
import { clearBrowseBridge, getBrowseBridge } from "../shared/browse-bridge";

function t(key: string): string {
  const dict: Record<string, string> = {
    "pathSelector.refresh": "刷新",
    "pathSelector.selectDirectory": "选择此目录",
    "pathSelector.emptyDirectory": "该目录为空",
    "pathSelector.colName": "名称",
    "pathSelector.colPermissions": "权限",
    "pathSelector.colSize": "大小",
    "pathSelector.colModified": "修改时间",
    "pathSelector.cannotAccess": "无法访问该目录",
    "common.cancel": "取消",
  };
  return dict[key] ?? key;
}

function formatLong(value: string | null | undefined): string {
  if (!value) return "";
  try {
    const d = new Date(value);
    if (Number.isNaN(d.getTime())) return "";
    return d.toLocaleString();
  } catch {
    return "";
  }
}

const vfsApi: FileBrowserVfsApi = {
  browse: (path, sourceId) => browseVfs(sourceId, path),
  stat: {
    stat: (paths, sourceId) => statVfs(paths, sourceId),
  },
};

export default function VfsBrowserWindow({ win }: { win: ShellWindowHandle }) {
  const bridgeId =
    typeof win.metadata.bridgeId === "string" ? win.metadata.bridgeId : "";
  const [bridge] = useState(() =>
    bridgeId ? getBrowseBridge(bridgeId) : undefined,
  );

  if (!bridge) return null;

  const finish = (path: string | null) => {
    bridge.resolve(path);
    clearBrowseBridge(bridgeId);
    win.close();
  };

  return (
    <ConfigProvider locale={zhCN}>
      <FileBrowserWindow
        initialPath={bridge.initialPath}
        sourceId={bridge.sourceId}
        protocolPrefix={bridge.protocolPrefix}
        vfsApi={vfsApi}
        t={t}
        formatLong={formatLong}
        onConfirm={(path) => finish(path)}
        onCancel={() => finish(null)}
      />
    </ConfigProvider>
  );
}
