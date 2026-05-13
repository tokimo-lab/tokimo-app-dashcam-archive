export const zhCN: Record<string, string> = {
  title: "录像归并",
  subtitle: "行车记录仪 / 监控视频按时间分组合并与转码",
  appName: "录像归并",
  appSubtitle: "行车记录仪 / 监控视频按时间分组合并与转码",

  // Form sections
  sectionBasic: "基本设置",
  sectionGrouping: "分组设置",
  sectionEncoder: "编码器",
  sectionTrigger: "触发器",
  sectionStatus: "状态",

  // Field labels
  fieldName: "名称",
  fieldSrcPath: "源路径",
  fieldDstPath: "目标路径",
  fieldMaxGap: "最大间隔（秒）",
  fieldMaxDuration: "最大分组时长（秒）",
  fieldMonthlySubdirs: "月度子目录",
  fieldAllowCombinedInput: "允许合并输入",
  fieldNoBrokenSplit: "禁止拆分",
  fieldEncoder: "编码器",
  fieldEncoderParams: "编码参数（JSON）",
  fieldTriggerMode: "触发模式",
  fieldCronExpr: "Cron 表达式",
  fieldWatcherDebounce: "监听防抖（秒）",
  fieldEnabled: "启用",

  // Placeholders
  placeholderName: "输入源名称",
  placeholderSrcPath: "/path/to/source",
  placeholderDstPath: "/path/to/destination",
  placeholderCronExpr: "0 */6 * * *",

  // Hints
  hintMaxGap: "超过此秒数视为新组",

  // Options
  optionAuto: "自动",
  optionOn: "开启",
  optionOff: "关闭",

  // Trigger modes
  triggerManualOnly: "仅手动",
  triggerCron: "定时",
  triggerWatcher: "监听",
  triggerCronWatcher: "定时+监听",

  // Encoder status
  available: "可用",
  unavailable: "不可用",

  // Buttons
  btnSave: "保存",
  btnDelete: "删除",
  btnCancel: "取消",
  btnBrowse: "浏览…",

  // Messages
  saveSuccess: "保存成功",
  deleteSuccess: "删除成功",
  errorSave: "保存失败",
  errorDelete: "删除失败",
  invalidJsonParams: "JSON 参数格式无效",
  confirmDelete: "确认删除此源？",

  // Encoder params
  showParams: "显示高级参数",
  hideParams: "隐藏高级参数",

  // Dir picker
  dirPickerTitle: "选择目录",
  confirmDirPick: "选择此目录",
  parentDir: "上级目录",
  loading: "加载中…",
  noDirectories: "无子目录",

  // Danger zone
  labelDangerZone: "危险操作",
};

export const enUS: Record<string, string> = {
  title: "Dashcam Archive",
  subtitle: "Group, merge, and transcode dashcam / surveillance footage by time",
  appName: "Dashcam Archive",
  appSubtitle: "Group, merge, and transcode dashcam / surveillance footage by time",

  // Form sections
  sectionBasic: "Basic Settings",
  sectionGrouping: "Grouping Settings",
  sectionEncoder: "Encoder",
  sectionTrigger: "Trigger",
  sectionStatus: "Status",

  // Field labels
  fieldName: "Name",
  fieldSrcPath: "Source Path",
  fieldDstPath: "Destination Path",
  fieldMaxGap: "Max Gap (seconds)",
  fieldMaxDuration: "Max Group Duration (seconds)",
  fieldMonthlySubdirs: "Monthly Subdirectories",
  fieldAllowCombinedInput: "Allow Combined Input",
  fieldNoBrokenSplit: "No Broken Split",
  fieldEncoder: "Encoder",
  fieldEncoderParams: "Encoder Parameters (JSON)",
  fieldTriggerMode: "Trigger Mode",
  fieldCronExpr: "Cron Expression",
  fieldWatcherDebounce: "Watcher Debounce (seconds)",
  fieldEnabled: "Enabled",

  // Placeholders
  placeholderName: "Enter source name",
  placeholderSrcPath: "/path/to/source",
  placeholderDstPath: "/path/to/destination",
  placeholderCronExpr: "0 */6 * * *",

  // Hints
  hintMaxGap: "Videos separated by more than this are grouped separately",

  // Options
  optionAuto: "Auto",
  optionOn: "On",
  optionOff: "Off",

  // Trigger modes
  triggerManualOnly: "Manual Only",
  triggerCron: "Cron",
  triggerWatcher: "Watcher",
  triggerCronWatcher: "Cron+Watcher",

  // Encoder status
  available: "Available",
  unavailable: "Unavailable",

  // Buttons
  btnSave: "Save",
  btnDelete: "Delete",
  btnCancel: "Cancel",
  btnBrowse: "Browse…",

  // Messages
  saveSuccess: "Saved successfully",
  deleteSuccess: "Deleted successfully",
  errorSave: "Failed to save",
  errorDelete: "Failed to delete",
  invalidJsonParams: "Invalid JSON parameters format",
  confirmDelete: "Are you sure you want to delete this source?",

  // Encoder params
  showParams: "Show Advanced Parameters",
  hideParams: "Hide Advanced Parameters",

  // Dir picker
  dirPickerTitle: "Select Directory",
  confirmDirPick: "Select This Directory",
  parentDir: "Parent Directory",
  loading: "Loading…",
  noDirectories: "No subdirectories",

  // Danger zone
  labelDangerZone: "Danger Zone",
};
