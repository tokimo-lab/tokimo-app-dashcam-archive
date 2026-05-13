/**
 * Settings form for creating/editing a source.
 */
import {
  Button,
  Input,
  InputNumber,
  Popconfirm,
  SegmentedControl,
  Select,
  SettingGroup,
  SettingRow,
  StickySaveBar,
  Switch,
  useToast,
} from "@tokimo/ui";
import { useEffect, useState } from "react";
import type { EncoderDto, SourceDto, SourceReq } from "./api";
import { createSource, deleteSource, getEncoders, updateSource } from "./api";
import { DirPicker } from "./dir-picker";

interface Props {
  source: SourceDto | "create";
  onSaved: () => void;
  onDeleted: () => void;
  t: (key: string) => string;
}

export function SourceForm({ source, onSaved, onDeleted, t }: Props) {
  const isCreate = source === "create";
  const toast = useToast();

  const initialData: SourceReq = isCreate
    ? {
        name: "",
        src_path: "",
        dst_path: "",
        encoder: "auto",
        encoder_params: {},
        max_gap_seconds: 60,
        max_group_duration_seconds: 7200,
        monthly_subdirs: "auto",
        allow_combined_input: false,
        no_broken_split: false,
        trigger_mode: "manual_only",
        cron_expr: "",
        watcher_debounce_secs: 60,
        enabled: true,
      }
    : {
        name: source.name,
        src_path: source.src_path ?? "",
        dst_path: source.dst_path ?? "",
        encoder: source.encoder,
        encoder_params: source.encoder_params,
        max_gap_seconds: source.max_gap_seconds,
        max_group_duration_seconds: source.max_group_duration_seconds,
        monthly_subdirs: source.monthly_subdirs,
        allow_combined_input: source.allow_combined_input,
        no_broken_split: source.no_broken_split,
        trigger_mode: source.trigger_mode,
        cron_expr: source.cron_expr ?? "",
        watcher_debounce_secs: source.watcher_debounce_secs,
        enabled: source.enabled,
      };

  const [data, setData] = useState<SourceReq>(initialData);
  const [encoders, setEncoders] = useState<EncoderDto[]>([]);
  const [paramsJson, setParamsJson] = useState(
    JSON.stringify(initialData.encoder_params, null, 2),
  );
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    getEncoders()
      .then(setEncoders)
      .catch((err) => console.error("Load encoders failed:", err));
  }, []);

  const handleSave = async () => {
    setSaving(true);
    try {
      let params: Record<string, unknown>;
      try {
        params = JSON.parse(paramsJson) as Record<string, unknown>;
      } catch {
        toast.error(t("invalidJsonParams"));
        setSaving(false);
        return;
      }
      const payload = { ...data, encoder_params: params };
      if (isCreate) {
        await createSource(payload);
      } else {
        await updateSource(source.id, payload);
      }
      toast.success(t("saveSuccess"));
      onSaved();
    } catch (err) {
      toast.error(
        `${t("errorSave")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (isCreate) return;
    setDeleting(true);
    try {
      await deleteSource(source.id);
      toast.success(t("deleteSuccess"));
      onDeleted();
    } catch (err) {
      toast.error(
        `${t("errorDelete")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    } finally {
      setDeleting(false);
    }
  };


  const monthlyOpts = [
    { value: "auto", label: t("optionAuto") },
    { value: "on", label: t("optionOn") },
    { value: "off", label: t("optionOff") },
  ];

  const triggerModeOpts = [
    { value: "manual_only", label: t("triggerManualOnly") },
    { value: "cron", label: t("triggerCron") },
    { value: "watcher", label: t("triggerWatcher") },
    { value: "cron+watcher", label: t("triggerCronWatcher") },
  ];

  const showCron =
    data.trigger_mode === "cron" || data.trigger_mode === "cron+watcher";
  const showWatcher =
    data.trigger_mode === "watcher" || data.trigger_mode === "cron+watcher";

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex-1 min-h-0 overflow-y-auto p-4">
        <SettingGroup title={t("sectionBasic")}>
          <SettingRow label={t("fieldName")}>
            <Input
              value={data.name}
              onChange={(e) => setData({ ...data, name: e.target.value })}
              placeholder={t("placeholderName")}
            />
          </SettingRow>
          <SettingRow label={t("fieldSrcPath")}>
            <DirPicker
              value={data.src_path ?? ""}
              onChange={(path) => setData({ ...data, src_path: path })}
              t={t}
            />
          </SettingRow>
          <SettingRow label={t("fieldDstPath")}>
            <DirPicker
              value={data.dst_path ?? ""}
              onChange={(path) => setData({ ...data, dst_path: path })}
              t={t}
            />
          </SettingRow>
        </SettingGroup>

        <SettingGroup title={t("sectionGrouping")}>
          <SettingRow label={t("fieldMaxGap")} desc={t("hintMaxGap")}>
            <InputNumber
              value={data.max_gap_seconds ?? 0}
              onChange={(val) =>
                setData({ ...data, max_gap_seconds: val ?? 0 })
              }
              min={0}
            />
          </SettingRow>
          <SettingRow label={t("fieldMaxDuration")}>
            <InputNumber
              value={data.max_group_duration_seconds ?? 0}
              onChange={(val) =>
                setData({ ...data, max_group_duration_seconds: val ?? 0 })
              }
              min={0}
            />
          </SettingRow>
          <SettingRow label={t("fieldMonthlySubdirs")} orientation="vertical">
            <SegmentedControl
              value={data.monthly_subdirs ?? "auto"}
              onChange={(val) =>
                setData({ ...data, monthly_subdirs: val as "auto" | "on" | "off" })
              }
              options={monthlyOpts}
            />
          </SettingRow>
          <SettingRow label={t("fieldAllowCombinedInput")}>
            <Switch
              checked={data.allow_combined_input ?? false}
              onChange={(val) =>
                setData({ ...data, allow_combined_input: val })
              }
            />
          </SettingRow>
          <SettingRow label={t("fieldNoBrokenSplit")}>
            <Switch
              checked={data.no_broken_split ?? false}
              onChange={(val) => setData({ ...data, no_broken_split: val })}
            />
          </SettingRow>
        </SettingGroup>

        <SettingGroup title={t("sectionEncoder")}>
          <SettingRow label={t("fieldEncoder")}>
            <Select
              value={data.encoder}
              onChange={(val) => setData({ ...data, encoder: val })}
              options={encoders.map((enc) => ({
                value: enc.id,
                label: enc.display_name,
                description: (
                  <span className="text-fg-muted text-xs">
                    {enc.description} •{" "}
                    <span
                      className={
                        enc.available ? "text-fg-success" : "text-fg-danger"
                      }
                    >
                      {t(enc.available ? "available" : "unavailable")}
                    </span>
                  </span>
                ),
                disabled: !enc.available,
              }))}
            />
          </SettingRow>
          <SettingRow label={t("fieldEncoderParams")} orientation="vertical">
            <textarea
              value={paramsJson}
              onChange={(e) => setParamsJson(e.target.value)}
              rows={8}
              className="bg-surface-elevated border-border-base text-fg-primary w-full rounded border p-2 font-mono text-xs"
            />
          </SettingRow>
        </SettingGroup>

        <SettingGroup title={t("sectionTrigger")}>
          <SettingRow
            label={t("fieldTriggerMode")}
            orientation="vertical"
            desc={data.trigger_mode === "manual_only" ? t("hintManualOnly") : undefined}
          >
            <SegmentedControl
              value={data.trigger_mode ?? "manual_only"}
              onChange={(val) =>
                setData({ ...data, trigger_mode: val as SourceReq["trigger_mode"] })
              }
              options={triggerModeOpts}
              className="w-full"
            />
          </SettingRow>
          {showCron && (
            <SettingRow label={t("fieldCronExpr")}>
              <Input
                value={data.cron_expr ?? ""}
                onChange={(e) =>
                  setData({ ...data, cron_expr: e.target.value })
                }
                placeholder={t("placeholderCronExpr")}
              />
            </SettingRow>
          )}
          {showWatcher && (
            <SettingRow label={t("fieldWatcherDebounce")}>
              <InputNumber
                value={data.watcher_debounce_secs ?? 0}
                onChange={(val) =>
                  setData({ ...data, watcher_debounce_secs: val ?? 0 })
                }
                min={0}
              />
            </SettingRow>
          )}
        </SettingGroup>

        <SettingGroup title={t("sectionStatus")}>
          <SettingRow label={t("fieldEnabled")}>
            <Switch
              checked={data.enabled ?? true}
              onChange={(val) => setData({ ...data, enabled: val })}
            />
          </SettingRow>
          {!isCreate && (
            <SettingRow label={t("labelDangerZone")}>
              <Popconfirm
                title={t("confirmDelete")}
                okType="danger"
                okText={t("btnDelete")}
                cancelText={t("btnCancel")}
                onConfirm={handleDelete}
              >
                <Button
                  variant="danger"
                  disabled={deleting}
                  loading={deleting}
                >
                  {t("btnDelete")}
                </Button>
              </Popconfirm>
            </SettingRow>
          )}
        </SettingGroup>

        <StickySaveBar
          dirty={true}
          loading={saving}
          onSave={handleSave}
          onReset={() => {
            setData(initialData);
            setParamsJson(JSON.stringify(initialData.encoder_params, null, 2));
          }}
          saveLabel={t("btnSave")}
          resetLabel={t("btnCancel")}
        />
      </div>
    </div>
  );
}
