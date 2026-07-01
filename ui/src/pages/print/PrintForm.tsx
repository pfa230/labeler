import { useState } from "react";
import { FieldForm, type FormValue } from "./FieldForm";
import { useLivePreview } from "../../lib/livePreview";
import { defaultOptions, reconcileRowOptions, referencedFields } from "../../lib/templateFields";
import { ApiError, fetchBlob, printLabel, saveBlob, submitBatch } from "../../api/client";
import { useToast } from "../../app/toast-context";
import type { BatchSummary, TemplateDetail } from "../../api/types";
import { PreviewPane } from "../../components/PreviewPane";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };

const buttonBase =
  "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

const MIN_COPIES = 1;
const MAX_COPIES = 100;
const clampCopies = (n: number) => Math.max(MIN_COPIES, Math.min(MAX_COPIES, Math.floor(Number.isFinite(n) ? n : 1)));

export function PrintForm({ detail, stale }: { detail: TemplateDetail; stale?: boolean }) {
  const [value, setValue] = useState<FormValue>(() => ({
    data: {},
    option: defaultOptions(detail.options),
    printer: undefined,
    startSlot: 0,
  }));
  const [fmt, setFmt] = useState<"png" | "pdf">("png");
  const [copies, setCopies] = useState(1);
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { push } = useToast();

  const showSummary = (summary: BatchSummary) => {
    const { succeeded, total, failed } = summary;
    const detailMsg = failed.length ? ` — ${failed[0].error}` : "";
    push({ kind: failed.length ? "error" : "ok", message: `Printed ${succeeded}/${total}${detailMsg}` });
  };

  const isSheet = detail.format.type === "sheet";
  const reconciledOption = reconcileRowOptions(value.option, detail.options);
  const fields = referencedFields(detail.layout, reconciledOption);
  const valid = fields.every((f) => (value.data[f] ?? "").length > 0);
  const hasOptions = !!detail.options && Object.keys(detail.options).length > 0;
  const option = hasOptions ? reconciledOption : undefined;
  const startSlot = isSheet ? value.startSlot : undefined;
  const label = { data: value.data, ...(option ? { option } : {}) };

  const preview = useLivePreview(
    { templateId: detail.id, format: detail.format.type, data: value.data, option, startSlot },
    valid,
  );

  const onDownload = async () => {
    setFormError(null);
    if (stale) return; // detail is the previous template during a switch (keepPreviousData); do not submit
    setBusy(true);
    try {
      if (isSheet) {
        const r = await submitBatch({
          template: detail.id,
          labels: [label],
          mode: "download",
          ...(startSlot ? { start_slot: startSlot } : {}),
        });
        if (r.kind === "download") saveBlob(r.blob, r.filename ?? `${detail.id}.pdf`);
      } else {
        const { blob, filename } = await fetchBlob(`/render/label?format=${fmt}`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ template: detail.id, data: value.data, ...(option ? { option } : {}) }),
        });
        saveBlob(blob, filename ?? `${detail.id}.${fmt}`);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : "Download failed";
      push({ kind: "error", message });
    } finally {
      setBusy(false);
    }
  };

  const onPrint = async () => {
    setFormError(null);
    if (stale) return; // detail is the previous template during a switch (keepPreviousData); do not submit
    setBusy(true);
    try {
      const n = clampCopies(copies);
      if (isSheet) {
        const r = await submitBatch({
          template: detail.id,
          labels: Array.from({ length: n }, () => label),
          mode: "print",
          printer: value.printer,
          ...(startSlot ? { start_slot: startSlot } : {}),
        });
        if (r.kind === "summary") showSummary(r.summary);
      } else {
        const summary = await printLabel({
          template: detail.id,
          printer: value.printer,
          fields: value.data,
          ...(option ? { option } : {}),
          copies: n,
        });
        showSummary(summary);
      }
    } catch (err) {
      if (err instanceof ApiError && err.code === "BatchInvalid") {
        const failures = (err.details as BatchFailures)?.failures ?? [];
        const message = failures.map((f) => f.message).join("; ") || err.message;
        setFormError(message);
        push({ kind: "error", message });
      } else {
        const message = err instanceof Error ? err.message : "Print failed";
        push({ kind: "error", message });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <div className="flex flex-col gap-4">
        <FieldForm detail={detail} value={{ ...value, option: reconciledOption }} onChange={setValue} />

        {formError && <p style={{ color: "var(--bad)" }}>{formError}</p>}

        {!isSheet && (
          <label className="flex items-center gap-2 text-sm">
            <span className="font-medium">Format</span>
            <select
              aria-label="download format"
              value={fmt}
              onChange={(e) => setFmt(e.target.value as "png" | "pdf")}
              className="rounded-md border px-2 py-1"
              style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
            >
              <option value="png">png</option>
              <option value="pdf">pdf</option>
            </select>
          </label>
        )}

        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1">
            <span className="text-sm font-medium">Copies</span>
            <button
              type="button"
              aria-label="decrease copies"
              onClick={() => setCopies((c) => clampCopies(c - 1))}
              className={`${buttonBase} border`}
              style={{ borderColor: "var(--border)", color: "var(--ink)" }}
            >
              −
            </button>
            <input
              type="number"
              aria-label="copies"
              min={MIN_COPIES}
              max={MAX_COPIES}
              value={copies}
              onChange={(e) => setCopies(clampCopies(Number(e.target.value)))}
              className="w-16 rounded-md border px-2 py-1 text-center"
              style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
            />
            <button
              type="button"
              aria-label="increase copies"
              onClick={() => setCopies((c) => clampCopies(c + 1))}
              className={`${buttonBase} border`}
              style={{ borderColor: "var(--border)", color: "var(--ink)" }}
            >
              +
            </button>
          </div>
          <button
            type="button"
            onClick={onPrint}
            disabled={busy || !value.printer || !valid || stale}
            className={buttonBase}
            style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
          >
            Print
          </button>
          <button
            type="button"
            onClick={onDownload}
            disabled={busy || !valid || stale}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Download
          </button>
        </div>
      </div>

      <PreviewPane name={detail.name} format={detail.format.type} preview={preview} />
    </div>
  );
}

