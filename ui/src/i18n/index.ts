// ⚠️ 不要在 ui/src/ 下创建 i18n.ts 文件！
// Vite/Node 解析 `./i18n` 时文件优先于目录，i18n.ts 会 shadow 这个 index.ts，
// 导致这里加的 key 全部 fallback 成 key 字符串（i18n shadow bug 已复发过 2 次）。
// 所有翻译必须加在本文件里；不要拆出去。

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
  fieldMaxGapDesc: "两段相邻视频被视为同一组的最大间隔（秒）。",
  fieldMaxGroupDurationDesc: "单组合并后总时长上限（秒）。0 表示不限制。",
  fieldAllowCombinedInputDesc:
    "允许把已经合并过的输出文件（含 _combined 后缀）作为下一轮的输入。默认关闭以避免重复处理。",
  fieldNoBrokenSplitDesc:
    "达到单组时长上限时不再切断当前剪辑，保留完整片段。开启可能让某些组略超时长上限。",

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
  btnClear: "清除",

  // Messages
  fieldRequiredName: "请填写任务名称",
  fieldRequiredSrcPath: "请选择源路径",
  fieldRequiredDstPath: "请选择目标路径",
  fieldRequiredSrcSource: "请选择源存储",
  fieldRequiredDstSource: "请选择目标存储",
  fieldRequiredCronExpr: "Cron 表达式不能为空",
  saveSuccess: "保存成功",
  deleteSuccess: "删除成功",
  errorSave: "保存失败",
  errorDelete: "删除失败",
  invalidJsonParams: "JSON 参数格式无效",
  confirmDelete: "确认删除此源？",

  // Encoder params
  showParams: "显示高级参数",
  hideParams: "隐藏高级参数",

  // Storage binding picker
  storageBindingPickerTitle: "选择存储位置",
  storageBindingPlaceholder: "请选择存储位置",
  remoteStorageSource: "远程",
  legacyStorageBinding: "（旧数据，请重新选择）",
  confirmDirPick: "选择此目录",
  parentDir: "上级目录",
  loading: "加载中…",
  noDirectories: "无子目录",

  // Danger zone
  labelDangerZone: "危险操作",

  // Dashboard
  dashboardTitle: "录像归并",
  dashboardSubtitle: "行车记录仪 / 监控视频归并中枢",
  totalSources: "源",
  enabledSources: "已启用",
  addSource: "+ 新增源",
  noSources: "暂无源",
  noSourcesAdd: "点击「+ 新增源」创建第一个归并任务",

  // Source card
  cardRunNow: "立即运行",
  cardViewHistory: "查看历史",
  cardStatusIdle: "空闲",
  cardStatusRunning: "运行中",
  cardStatusQueued: "排队",
  cardStatusSucceeded: "已完成",
  cardStatusFailed: "失败",
  cardStatusCancelled: "已取消",
  cardNoRuns: "尚未运行",
  cardLastRun: "上次",
  cardEta: "ETA",
  cardGroupsDone: "已处理",
  cardRunning: "处理中",
  cardCancelRun: "取消",

  // Settings modal / drawer
  modalEditSource: "编辑源",
  modalNewSource: "新建源",
  modalHistoryTitle: "运行历史",
  modalClose: "关闭",

  // Navigation keys (legacy, preserved)
  selectSource: "请选择一个源",
  selectSourceDesc: "或点击「新增源」创建新任务",
  tabSettings: "设置",
  tabRun: "运行",
  tabHistory: "历史",
  newSource: "+ 新增源",
  noSourcesDesc: "暂无归并源",
  errorLoad: "加载失败",

  // Misc
  emptyValue: "—",
  separatorFlow: "→",
  separatorMeta: "·",
  separatorLabel: ":",
  separatorLabelValue: ": ",
  separatorTimeRange: "~",
  separatorWarningCountPrefix: " × ",
  separatorWarningCountSuffix: "",
  separatorWarningExample: "，例：",
  separatorUnitValue: " ",
  separatorDurationParts: " ",
  unitByte: "B",
  unitKilobyte: "KB",
  unitMegabyte: "MB",
  unitGigabyte: "GB",
  unitTerabyte: "TB",
  unitSecondShort: "s",
  unitMinuteShort: "m",
  unitHourShort: "h",
  noHistory: "暂无运行记录",
  noGroups: "无分组",
  phase: "阶段",
  progress: "进度",
  currentFile: "当前文件",
  okCount: "成功",
  failedCount: "失败",
  totalGroups: "总组数",
  okGroups: "成功组",
  failedGroupsLabel: "失败组",
  groupCamera: "摄像头",
  groupOutput: "输出",
  groupDecision: "决策",
  groupStatus: "状态",
  abortLabel: "中断原因",
  warnings: "警告",
  warningLevelClean: "正常",
  warningLevelWarn: "警告",
  warningLevelSuspicious: "可疑",
  warningLevelFatal: "致命",
  statusQueued: "排队中",
  statusRunning: "运行中",
  statusSucceeded: "已完成",
  statusFailed: "失败",
  statusCancelled: "已取消",
  triggerManual: "手动",
  triggerCron2: "定时",
  triggerWatcher2: "监听",
  runStarted: "运行已启动",
  runCancelled: "运行已取消",
  errorRun: "运行失败",
  errorCancel: "取消失败",
  btnRunNow: "立即运行",
  hintManualOnly: "",
};

export const enUS: Record<string, string> = {
  title: "Dashcam Archive",
  subtitle:
    "Group, merge, and transcode dashcam / surveillance footage by time",
  appName: "Dashcam Archive",
  appSubtitle:
    "Group, merge, and transcode dashcam / surveillance footage by time",

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
  fieldMaxGapDesc:
    "Max gap between two adjacent clips to still belong to the same group (seconds).",
  fieldMaxGroupDurationDesc:
    "Upper bound of a merged group total duration (seconds). 0 means unlimited.",
  fieldAllowCombinedInputDesc:
    "Allow already-merged outputs (with _combined suffix) as input for next run. Off by default to prevent re-processing.",
  fieldNoBrokenSplitDesc:
    "When max group duration is hit, do NOT split the current clip; keep it whole. May cause some groups to slightly exceed the limit.",

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
  btnClear: "Clear",

  // Messages
  fieldRequiredName: "Name is required",
  fieldRequiredSrcPath: "Source path is required",
  fieldRequiredDstPath: "Destination path is required",
  fieldRequiredSrcSource: "Source storage is required",
  fieldRequiredDstSource: "Destination storage is required",
  fieldRequiredCronExpr: "Cron expression is required",
  saveSuccess: "Saved successfully",
  deleteSuccess: "Deleted successfully",
  errorSave: "Failed to save",
  errorDelete: "Failed to delete",
  invalidJsonParams: "Invalid JSON parameters format",
  confirmDelete: "Are you sure you want to delete this source?",

  // Encoder params
  showParams: "Show Advanced Parameters",
  hideParams: "Hide Advanced Parameters",

  // Storage binding picker
  storageBindingPickerTitle: "Select Storage Location",
  storageBindingPlaceholder: "Select a storage location",
  remoteStorageSource: "Remote",
  legacyStorageBinding: "(legacy data, please reselect)",
  confirmDirPick: "Select This Directory",
  parentDir: "Parent Directory",
  loading: "Loading…",
  noDirectories: "No subdirectories",

  // Danger zone
  labelDangerZone: "Danger Zone",

  // Dashboard
  dashboardTitle: "Dashcam Archive",
  dashboardSubtitle: "Dashcam / surveillance footage merge hub",
  totalSources: "Sources",
  enabledSources: "Enabled",
  addSource: "+ Add Source",
  noSources: "No sources",
  noSourcesAdd: 'Click "+ Add Source" to create your first merge task',

  // Source card
  cardRunNow: "Run Now",
  cardViewHistory: "History",
  cardStatusIdle: "Idle",
  cardStatusRunning: "Running",
  cardStatusQueued: "Queued",
  cardStatusSucceeded: "Completed",
  cardStatusFailed: "Failed",
  cardStatusCancelled: "Cancelled",
  cardNoRuns: "No runs yet",
  cardLastRun: "Last",
  cardEta: "ETA",
  cardGroupsDone: "Processed",
  cardRunning: "Processing",
  cardCancelRun: "Cancel",

  // Settings modal / drawer
  modalEditSource: "Edit Source",
  modalNewSource: "New Source",
  modalHistoryTitle: "Run History",
  modalClose: "Close",

  // Navigation keys (legacy, preserved)
  selectSource: "Select a source",
  selectSourceDesc: 'Or click "Add Source" to create a new task',
  tabSettings: "Settings",
  tabRun: "Run",
  tabHistory: "History",
  newSource: "+ Add Source",
  noSourcesDesc: "No merge sources",
  errorLoad: "Failed to load",

  // Misc
  emptyValue: "—",
  separatorFlow: "→",
  separatorMeta: "·",
  separatorLabel: ":",
  separatorLabelValue: ": ",
  separatorTimeRange: "~",
  separatorWarningCountPrefix: " × ",
  separatorWarningCountSuffix: "",
  separatorWarningExample: ", e.g.: ",
  separatorUnitValue: " ",
  separatorDurationParts: " ",
  unitByte: "B",
  unitKilobyte: "KB",
  unitMegabyte: "MB",
  unitGigabyte: "GB",
  unitTerabyte: "TB",
  unitSecondShort: "s",
  unitMinuteShort: "m",
  unitHourShort: "h",
  noHistory: "No run history",
  noGroups: "No groups",
  phase: "Phase",
  progress: "Progress",
  currentFile: "Current file",
  okCount: "OK",
  failedCount: "Failed",
  totalGroups: "Total groups",
  okGroups: "OK groups",
  failedGroupsLabel: "failed groups",
  groupCamera: "Camera",
  groupOutput: "Output",
  groupDecision: "Decision",
  groupStatus: "Status",
  abortLabel: "Abort reason",
  warnings: "Warnings",
  warningLevelClean: "Clean",
  warningLevelWarn: "Warn",
  warningLevelSuspicious: "Suspicious",
  warningLevelFatal: "Fatal",
  statusQueued: "Queued",
  statusRunning: "Running",
  statusSucceeded: "Succeeded",
  statusFailed: "Failed",
  statusCancelled: "Cancelled",
  triggerManual: "Manual",
  triggerCron2: "Cron",
  triggerWatcher2: "Watcher",
  runStarted: "Run started",
  runCancelled: "Run cancelled",
  errorRun: "Run failed",
  errorCancel: "Cancel failed",
  btnRunNow: "Run Now",
  hintManualOnly: "",
};
