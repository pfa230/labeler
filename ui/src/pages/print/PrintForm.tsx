import { useState } from "react";
import { FieldForm, type FormValue } from "./FieldForm";
import { useLivePreview } from "../../lib/livePreview";
import { defaultOptions, referencedFields } from "../../lib/templateFields";
import { ApiError, fetchBlob, saveBlob, submitBatch } from "../../api/client";
import { useToast } from "../../app/toast-context";
import type { TemplateDetail, TemplateFormat } from "../../api/types";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };

const buttonBase =
  "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

export function PrintForm({ detail }: { detail: TemplateDetail }) {
  const [value, setValue] = useState<FormValue>(() => ({
    data: {},
    option: defaultOptions(detail.options),
    printer: undefined,
    startSlot: 0,
  }));
  const [fmt, setFmt] = useState<"png" | "pdf">("png");
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { push } = useToast();

  const isSheet = detail.format.type === "sheet";
  const fields = referencedFields(detail.layout, value.option);
  const valid = fields.every((f) => (value.data[f] ?? "").length > 0);
  const hasOptions = !!detail.options && Object.keys(detail.options).length > 0;
  const option = hasOptions ? value.option : undefined;
  const startSlot = isSheet ? value.startSlot : undefined;
  const label = { data: value.data, ...(option ? { option } : {}) };

  const preview = useLivePreview(
    { templateId: detail.id, format: detail.format.type, data: value.data, option, startSlot },
    valid,
  );

  const onDownload = async () => {
    setFormError(null);
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
        const { blob } = await fetchBlob(`/render/label?format=${fmt}`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ template: detail.id, data: value.data, ...(option ? { option } : {}) }),
        });
        saveBlob(blob, `${detail.id}.${fmt}`);
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
    setBusy(true);
    try {
      const r = await submitBatch({
        template: detail.id,
        labels: [label],
        mode: "print",
        printer: value.printer,
        ...(startSlot ? { start_slot: startSlot } : {}),
      });
      if (r.kind === "summary") {
        const { succeeded, total, failed } = r.summary;
        const detailMsg = failed.length ? ` — ${failed[0].error}` : "";
        push({ kind: failed.length ? "error" : "ok", message: `Printed ${succeeded}/${total}${detailMsg}` });
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
        <FieldForm detail={detail} value={value} onChange={setValue} />

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
          <button
            type="button"
            onClick={onPrint}
            disabled={busy || !value.printer || !valid}
            className={buttonBase}
            style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
          >
            Print
          </button>
          <button
            type="button"
            onClick={onDownload}
            disabled={busy || !valid}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Download
          </button>
        </div>
      </div>

      <PreviewPane detail={detail} preview={preview} />
    </div>
  );
}

function PreviewPane({
  detail,
  preview,
}: {
  detail: TemplateDetail;
  preview: { url?: string; error?: string; loading: boolean };
}) {
  const isSheet = (detail.format as TemplateFormat).type === "sheet";
  return (
    <div
      className="flex min-h-48 items-center justify-center rounded-lg border p-4"
      style={{ background: "var(--bg)", borderColor: "var(--border)" }}
    >
      {preview.loading && <p style={{ color: "var(--muted)" }}>rendering preview…</p>}
      {!preview.loading && preview.error && (
        <p style={{ color: "var(--bad)" }}>Preview failed: {preview.error}</p>
      )}
      {!preview.loading && !preview.error && preview.url && !isSheet && (
        <img src={preview.url} alt={`${detail.name} preview`} className="max-h-96 max-w-full" />
      )}
      {!preview.loading && !preview.error && preview.url && isSheet && (
        <object data={preview.url} type="application/pdf" className="h-96 w-full" aria-label={`${detail.name} preview`}>
          <a href={preview.url}>Open sheet preview</a>
        </object>
      )}
      {!preview.loading && !preview.error && !preview.url && (
        <p style={{ color: "var(--muted)" }}>Fill the required fields to preview.</p>
      )}
    </div>
  );
}
