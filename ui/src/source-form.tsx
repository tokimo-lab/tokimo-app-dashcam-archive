/**
 * Settings form for creating/editing a source.
 */
import { Button, Input, Select, Switch } from "@tokimo/ui";
import { useEffect, useId, useState } from "react";
import type { EncoderDto, SourceDto, SourceReq } from "./api";
import { createSource, deleteSource, getEncoders, updateSource } from "./api";

interface Props {
  source: SourceDto | "create";
  onSaved: () => void;
  onDeleted: () => void;
  t: (key: string) => string;
}

export function SourceForm({ source, onSaved, onDeleted, t }: Props) {
  const isCreate = source === "create";
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
  const [showParams, setShowParams] = useState(false);
  const [paramsJson, setParamsJson] = useState(
    JSON.stringify(initialData.encoder_params, null, 2),
  );
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const formId = useId();
  const nameId = `${formId}-name`;
  const srcPathId = `${formId}-src-path`;
  const dstPathId = `${formId}-dst-path`;
  const maxGapId = `${formId}-max-gap`;
  const maxDurationId = `${formId}-max-duration`;
  const monthlySubdirsId = `${formId}-monthly-subdirs`;
  const allowCombinedInputId = `${formId}-allow-combined-input`;
  const noBrokenSplitId = `${formId}-no-broken-split`;
  const encoderId = `${formId}-encoder`;
  const encoderParamsId = `${formId}-encoder-params`;
  const triggerModeId = `${formId}-trigger-mode`;
  const cronExprId = `${formId}-cron-expr`;
  const watcherDebounceId = `${formId}-watcher-debounce`;
  const enabledId = `${formId}-enabled`;

  useEffect(() => {
    getEncoders()
      .then(setEncoders)
      .catch((err) => console.error("Load encoders failed:", err));
  }, []);

  const handleSave = async () => {
    setSaving(true);
    try {
      let params = data.encoder_params;
      if (showParams) {
        try {
          params = JSON.parse(paramsJson) as Record<string, unknown>;
        } catch {
          alert(t("invalidJsonParams"));
          setSaving(false);
          return;
        }
      }
      const payload = { ...data, encoder_params: params };
      if (isCreate) {
        await createSource(payload);
      } else {
        await updateSource(source.id, payload);
      }
      alert(t("saveSuccess"));
      onSaved();
    } catch (err) {
      alert(
        `${t("errorSave")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!window.confirm(t("confirmDelete"))) return;
    if (isCreate) return;
    setDeleting(true);
    try {
      await deleteSource(source.id);
      alert(t("deleteSuccess"));
      onDeleted();
    } catch (err) {
      alert(
        `${t("errorDelete")}: ${err instanceof Error ? err.message : String(err)}`,
      );
    } finally {
      setDeleting(false);
    }
  };

  const triggerModeOpts = [
    { value: "manual_only", label: t("triggerManualOnly") },
    { value: "cron", label: t("triggerCron") },
    { value: "watcher", label: t("triggerWatcher") },
    { value: "cron+watcher", label: t("triggerCronWatcher") },
  ];

  const monthlyOpts = [
    { value: "auto", label: t("optionAuto") },
    { value: "on", label: t("optionOn") },
    { value: "off", label: t("optionOff") },
  ];

  const showCron =
    data.trigger_mode === "cron" || data.trigger_mode === "cron+watcher";
  const showWatcher =
    data.trigger_mode === "watcher" || data.trigger_mode === "cron+watcher";

  return (
    <div className="space-y-6 p-4">
      {/* Basic */}
      <section>
        <h3 className="text-fg-primary mb-3 text-sm font-semibold">
          {t("sectionBasic")}
        </h3>
        <div className="space-y-3">
          <div>
            <label
              htmlFor={nameId}
              className="text-fg-secondary mb-1 block text-xs"
            >
              {t("fieldName")}
            </label>
            <Input
              id={nameId}
              value={data.name}
              onChange={(e) => setData({ ...data, name: e.target.value })}
              placeholder={t("placeholderName")}
            />
          </div>
          <div>
            <label
              htmlFor={srcPathId}
              className="text-fg-secondary mb-1 block text-xs"
            >
              {t("fieldSrcPath")}
            </label>
            <Input
              id={srcPathId}
              value={data.src_path}
              onChange={(e) => setData({ ...data, src_path: e.target.value })}
              placeholder={t("placeholderSrcPath")}
            />
          </div>
          <div>
            <label
              htmlFor={dstPathId}
              className="text-fg-secondary mb-1 block text-xs"
            >
              {t("fieldDstPath")}
            </label>
            <Input
              id={dstPathId}
              value={data.dst_path}
              onChange={(e) => setData({ ...data, dst_path: e.target.value })}
              placeholder={t("placeholderDstPath")}
            />
          </div>
        </div>
      </section>

      {/* Grouping */}
      <section>
        <h3 className="text-fg-primary mb-3 text-sm font-semibold">
          {t("sectionGrouping")}
        </h3>
        <div className="space-y-3">
          <div>
            <label
              htmlFor={maxGapId}
              className="text-fg-secondary mb-1 block text-xs"
            >
              {t("fieldMaxGap")}
            </label>
            <Input
              id={maxGapId}
              type="number"
              value={data.max_gap_seconds}
              onChange={(e) =>
                setData({ ...data, max_gap_seconds: Number(e.target.value) })
              }
            />
            <p className="text-fg-muted mt-1 text-xs">{t("hintMaxGap")}</p>
          </div>
          <div>
            <label
              htmlFor={maxDurationId}
              className="text-fg-secondary mb-1 block text-xs"
            >
              {t("fieldMaxDuration")}
            </label>
            <Input
              id={maxDurationId}
              type="number"
              value={data.max_group_duration_seconds}
              onChange={(e) =>
                setData({
                  ...data,
                  max_group_duration_seconds: Number(e.target.value),
                })
              }
            />
          </div>
          <fieldset>
            <legend
              id={monthlySubdirsId}
              className="text-fg-secondary mb-1 block text-xs"
            >
              {t("fieldMonthlySubdirs")}
            </legend>
            <div className="flex gap-2">
              {monthlyOpts.map((opt) => (
                <button
                  key={opt.value}
                  type="button"
                  onClick={() =>
                    setData({
                      ...data,
                      monthly_subdirs: opt.value as "auto" | "on" | "off",
                    })
                  }
                  className={`
                    flex-1 cursor-pointer rounded border px-3 py-2 text-xs transition-colors
                    ${
                      data.monthly_subdirs === opt.value
                        ? "bg-accent border-accent text-fg-on-accent"
                        : "bg-surface-elevated border-border-base text-fg-secondary hover:bg-surface-glass"
                    }
                  `}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          </fieldset>
          <fieldset className="flex items-center justify-between">
            <legend
              id={allowCombinedInputId}
              className="text-fg-secondary text-xs"
            >
              {t("fieldAllowCombinedInput")}
            </legend>
            <Switch
              checked={data.allow_combined_input}
              onChange={(val) =>
                setData({ ...data, allow_combined_input: val })
              }
            />
          </fieldset>
          <fieldset className="flex items-center justify-between">
            <legend id={noBrokenSplitId} className="text-fg-secondary text-xs">
              {t("fieldNoBrokenSplit")}
            </legend>
            <Switch
              checked={data.no_broken_split}
              onChange={(val) => setData({ ...data, no_broken_split: val })}
            />
          </fieldset>
        </div>
      </section>

      {/* Encoder */}
      <section>
        <h3 className="text-fg-primary mb-3 text-sm font-semibold">
          {t("sectionEncoder")}
        </h3>
        <div className="space-y-3">
          <div>
            <fieldset>
              <legend
                id={encoderId}
                className="text-fg-secondary mb-1 block text-xs"
              >
                {t("fieldEncoder")}
              </legend>
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
            </fieldset>
          </div>
          <div>
            <button
              type="button"
              onClick={() => setShowParams(!showParams)}
              className="text-accent mb-2 cursor-pointer text-xs underline"
            >
              {t(showParams ? "hideParams" : "showParams")}
            </button>
            {showParams && (
              <>
                <label
                  htmlFor={encoderParamsId}
                  className="text-fg-secondary mb-1 block text-xs"
                >
                  {t("fieldEncoderParams")}
                </label>
                <textarea
                  id={encoderParamsId}
                  value={paramsJson}
                  onChange={(e) => setParamsJson(e.target.value)}
                  rows={8}
                  className="bg-surface-elevated border-border-base text-fg-primary w-full rounded border p-2 font-mono text-xs"
                />
              </>
            )}
          </div>
        </div>
      </section>

      {/* Trigger */}
      <section>
        <h3 className="text-fg-primary mb-3 text-sm font-semibold">
          {t("sectionTrigger")}
        </h3>
        <div className="space-y-3">
          <div>
            <fieldset>
              <legend
                id={triggerModeId}
                className="text-fg-secondary mb-1 block text-xs"
              >
                {t("fieldTriggerMode")}
              </legend>
              <div className="grid grid-cols-2 gap-2">
                {triggerModeOpts.map((opt) => (
                  <button
                    key={opt.value}
                    type="button"
                    onClick={() =>
                      setData({
                        ...data,
                        trigger_mode: opt.value as SourceReq["trigger_mode"],
                      })
                    }
                    className={`
                    cursor-pointer rounded border px-3 py-2 text-xs transition-colors
                    ${
                      data.trigger_mode === opt.value
                        ? "bg-accent border-accent text-fg-on-accent"
                        : "bg-surface-elevated border-border-base text-fg-secondary hover:bg-surface-glass"
                    }
                  `}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
            </fieldset>
          </div>
          {showCron && (
            <div>
              <label
                htmlFor={cronExprId}
                className="text-fg-secondary mb-1 block text-xs"
              >
                {t("fieldCronExpr")}
              </label>
              <Input
                id={cronExprId}
                value={data.cron_expr}
                onChange={(e) =>
                  setData({ ...data, cron_expr: e.target.value })
                }
                placeholder={t("placeholderCronExpr")}
              />
            </div>
          )}
          {showWatcher && (
            <div>
              <label
                htmlFor={watcherDebounceId}
                className="text-fg-secondary mb-1 block text-xs"
              >
                {t("fieldWatcherDebounce")}
              </label>
              <Input
                id={watcherDebounceId}
                type="number"
                value={data.watcher_debounce_secs}
                onChange={(e) =>
                  setData({
                    ...data,
                    watcher_debounce_secs: Number(e.target.value),
                  })
                }
              />
            </div>
          )}
        </div>
      </section>

      {/* Status */}
      <section>
        <h3 className="text-fg-primary mb-3 text-sm font-semibold">
          {t("sectionStatus")}
        </h3>
        <fieldset className="flex items-center justify-between">
          <legend id={enabledId} className="text-fg-secondary text-xs">
            {t("fieldEnabled")}
          </legend>
          <Switch
            checked={data.enabled}
            onChange={(val) => setData({ ...data, enabled: val })}
          />
        </fieldset>
      </section>

      {/* Actions */}
      <div className="flex gap-3">
        <Button onClick={handleSave} disabled={saving} className="flex-1">
          {t("btnSave")}
        </Button>
        {!isCreate && (
          <Button
            onClick={handleDelete}
            disabled={deleting}
            variant="danger"
            className="flex-1"
          >
            {t("btnDelete")}
          </Button>
        )}
      </div>
    </div>
  );
}
