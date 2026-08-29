import { useMemo, useState } from "react";
import { FieldForm, type FormValue } from "./FieldForm";
import { useLivePreview } from "../../lib/livePreview";
import { useMediaQuery } from "../../lib/useMediaQuery";
import { useLabelInputs, pruneDataForSubmit, getOwnKey, hasOwnKey, setOwnKey } from "../../lib/labelInputs";
import { ApiError, fetchBlob, printLabel, saveBlob, submitBatch } from "../../api/client";
import { usePrinters } from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { BatchSummary, InputSpec, ParamValue, TemplateDetail } from "../../api/types";
import { PreviewPane } from "../../components/PreviewPane";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };

const buttonBase =
  "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

const MIN_COPIES = 1;
const MAX_COPIES = 100;
const clampCopies = (n: number) => Math.max(MIN_COPIES, Math.min(MAX_COPIES, Math.floor(Number.isFinite(n) ? n : 1)));

// Every entry publishing a default is seeded from it and arrives deferred: the template decides it
// until an operator says otherwise. An entry publishing none is absent from both maps, which is not
// the same as holding an empty value or a `false` deferral.
function initialFieldState(inputs: InputSpec[]): Pick<FormValue, "data" | "deferred"> {
  const data: Record<string, ParamValue> = {};
  const deferred: Record<string, boolean> = {};
  for (const input of inputs) {
    if (input.default !== undefined && input.default !== null) {
      setOwnKey(data, input.name, input.default);
      setOwnKey(deferred, input.name, true);
    }
  }
  return { data, deferred };
}

// Deferral follows the entry, not the position. An entry a later list brings in for the first time
// is seeded and deferred here, exactly as one present at first paint; an entry already known keeps
// whatever value and deferral it had, which is what restores them when it returns.
function withArrivals(value: FormValue, inputs: InputSpec[]): FormValue {
  let data = value.data;
  let deferred = value.deferred;
  for (const input of inputs) {
    if (input.default === undefined || input.default === null) continue;
    if (hasOwnKey(deferred, input.name)) continue;
    if (deferred === value.deferred) deferred = { ...deferred };
    setOwnKey(deferred, input.name, true);
    if (hasOwnKey(data, input.name)) continue;
    if (data === value.data) data = { ...data };
    setOwnKey(data, input.name, input.default);
  }
  return deferred === value.deferred ? value : { ...value, data, deferred };
}

export function PrintForm({ detail, stale }: { detail: TemplateDetail; stale?: boolean }) {
  const [value, setValue] = useState<FormValue>(() => ({
    ...initialFieldState(detail.inputs?.default ?? []),
    printer: undefined,
    startSlot: 0,
  }));

  // Selecting a different template reinitialises BOTH values and deferral from the new template's
  // list. The retention rule governs branch changes within one template only: a name both templates
  // declare must carry nothing across, or template A's value would sit in a disabled control while
  // the render resolved B's default.
  const [renderedTemplateId, setRenderedTemplateId] = useState(detail.id);
  if (renderedTemplateId !== detail.id) {
    setRenderedTemplateId(detail.id);
    setValue((prev) => ({ ...prev, ...initialFieldState(detail.inputs?.default ?? []) }));
  }
  const [fmt, setFmt] = useState<"png" | "pdf">("png");
  const [copies, setCopies] = useState(1);
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { push } = useToast();

  const isLg = useMediaQuery("(min-width: 1024px)");
  const [previewOpen, setPreviewOpen] = useState(false);

  // The list is requested for the label this form would actually submit: the same pruning, and no
  // name it is deferring. A deferred name reaches the service as an omission here exactly as it
  // will at render time, so the branch the list reports is the branch the render takes.
  const { inputs, pending: inputsPending, error: inputsError } = useLabelInputs(
    detail.id,
    (currentInputs) => pruneDataForSubmit(value.data, currentInputs, value.deferred),
    detail.inputs?.default ?? [],
  );

  // Only a list that answers the values the form now holds can say which entries are present: while
  // one is in flight the previous list is still rendered, and `useLabelInputs` reports a list only for
  // the template it was requested for, so one template's entries can never seed another's, on the
  // failure path included.
  const form = inputsPending ? value : withArrivals(value, inputs);
  if (form !== value) setValue(form);

  // Printer preselect, derived at render (no effect; #116): default -> sole printer -> none.
  // `value.printer` stores only EXPLICIT user choices ("" = explicit None, an id = explicit pick,
  // undefined = untouched -> use the preselect), so a printers refetch never clobbers a choice.
  const { data: printers } = usePrinters();
  const preselect = useMemo(() => {
    const all = printers ?? [];
    return all.find((p) => p.is_default)?.id ?? (all.length === 1 ? all[0].id : undefined);
  }, [printers]);
  const effectivePrinter = form.printer === undefined ? preselect : form.printer || undefined;

  const showSummary = (summary: BatchSummary) => {
    const { succeeded, total, failed } = summary;
    const detailMsg = failed.length ? ` — ${failed[0].error}` : "";
    push({ kind: failed.length ? "error" : "ok", message: `Printed ${succeeded}/${total}${detailMsg}` });
  };

  const isSheet = detail.format.type === "sheet";
  const valid =
    !inputsPending &&
    inputs.every((input) => {
      if (!input.required) return true;
      const current = getOwnKey(form.data, input.name);
      return current !== undefined && current !== "" && current !== null;
    });

  const startSlot = isSheet ? form.startSlot : undefined;
  const submittedData = pruneDataForSubmit(form.data, inputs, form.deferred);
  const label = { data: submittedData };

  const preview = useLivePreview(
    { templateId: detail.id, format: detail.format.type, data: submittedData, startSlot },
    valid && (isLg || previewOpen),
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
          body: JSON.stringify({ template: detail.id, data: submittedData }),
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
    // Print requires a printer (the button is already gated on it); narrows to string.
    const printer = effectivePrinter;
    if (!printer) return;
    setBusy(true);
    try {
      const n = clampCopies(copies);
      if (isSheet) {
        const r = await submitBatch({
          template: detail.id,
          labels: Array.from({ length: n }, () => label),
          mode: "print",
          printer,
          ...(startSlot ? { start_slot: startSlot } : {}),
        });
        if (r.kind === "summary") showSummary(r.summary);
      } else {
        const summary = await printLabel({
          template: detail.id,
          printer,
          fields: submittedData,
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
        <FieldForm detail={detail} inputs={inputs} value={{ ...form, printer: effectivePrinter }} onChange={setValue} />

        {inputsError && <p style={{ color: "var(--bad)" }}>{inputsError}</p>}
        {formError && <p style={{ color: "var(--bad)" }}>{formError}</p>}

        <div className="flex items-center gap-3">
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

        <details className="lg:hidden" onToggle={(e) => setPreviewOpen(e.currentTarget.open)}>
          <summary className="cursor-pointer py-2 text-sm font-medium">Preview</summary>
          <PreviewPane name={detail.name} format={detail.format.type} preview={preview} />
        </details>

        <div
          className="sticky bottom-0 z-10 -mx-2 flex flex-wrap items-center gap-2 border-t px-2 py-3 lg:static lg:mx-0 lg:gap-3 lg:border-t-0 lg:px-0"
          style={{
            background: "var(--surface)",
            borderColor: "var(--border)",
            paddingBottom: "calc(0.75rem + env(safe-area-inset-bottom))",
          }}
        >
          <div className="flex items-center gap-1">
            <span className="text-sm font-medium">Copies</span>
            <button
              type="button"
              aria-label="decrease copies"
              onClick={() => setCopies((c) => clampCopies(c - 1))}
              className={`${buttonBase} h-11 w-11 border`}
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
              className="h-11 w-16 rounded-md border px-2 py-1 text-center"
              style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
            />
            <button
              type="button"
              aria-label="increase copies"
              onClick={() => setCopies((c) => clampCopies(c + 1))}
              className={`${buttonBase} h-11 w-11 border`}
              style={{ borderColor: "var(--border)", color: "var(--ink)" }}
            >
              +
            </button>
          </div>
          <button
            type="button"
            onClick={onPrint}
            disabled={busy || !effectivePrinter || !valid || stale}
            className={`${buttonBase} h-11 min-w-32 flex-1 lg:flex-none`}
            style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
          >
            Print
          </button>
        </div>
      </div>

      <div className="hidden lg:block">
        <PreviewPane name={detail.name} format={detail.format.type} preview={preview} />
      </div>
    </div>
  );
}

