// StorageBinding model used by dashcam-archive.
//
// A binding identifies a directory inside a host-registered VFS source:
//   - sourceId / sourceType / sourceName come from the host's `/api/vfs`
//     registry (`VfsDto`),
//   - path is the in-source directory selected by the user via DirPicker.
//
// The previous version also exported `hasStorageBindingPicker` which probed
// the host SDK for a now-removed `pickStorageBinding` modal API. It has been
// replaced by an in-sidecar picker (see `./dir-picker.tsx`) and is gone.

export interface StorageBinding {
  sourceId: string;
  sourceType: string;
  sourceName: string;
  displayHints?: { protocolPrefix?: string; rootPath?: string };
  path: string;
}
