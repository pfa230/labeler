import { useMemo, useState } from "react";
import { FieldForm, type FormValue } from "./FieldForm";
import { useLivePreview } from "../../lib/livePreview";
import { useMediaQuery } from "../../lib/useMediaQuery";
import { pruneDataForSubmit, setOwnKey, seedDefaultValue } from "../../lib/labelInputs";
import { ApiError, fetchBlob, printLabel, saveBlob, submitBatch } from "../../api/client";
import { usePrinters, useSettings } from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { BatchSummary, Param, ParamValue, TemplateDetail } from "../../api/types";
import { PreviewPane } from "../../components/PreviewPane";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };

const buttonBase =
  "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

const MIN_COPIES = 1;
const MAX_COPIES = 100;
const clampCopies = (n: number) => Math.max(MIN_COPIES, Math.min(MAX_COPIES, Math.floor(Number.isFinite(n) ? n : 1)));

// A checkbox starts at its published default, else unchecked, and is never deferred. Any other
// parameter publishing a default is seeded from it and starts deferred: the template decides it until
// an operator says otherwise. An undefaulted list seeds an empty array into data so the untouched
// editor is submittable, without being deferred. Any other parameter publishing no default is absent
// from both maps, which is not the same as holding an empty value or a `false` deferral.
function initialFieldState(params: Param[]): Pick<FormValue, "data" | "deferred"> {
  const data: Record<string, ParamValue> = {};
  const deferred: Record<string, boolean> = {};
  for (const param of params) {
    if (param.control === "checkbox") {
      setOwnKey(data, param.name, param.default === true);
    } else if (param.default !== undefined && param.default !== null) {
      setOwnKey(data, param.name, seedDefaultValue(param));
      setOwnKey(deferred, param.name, true);
    } else if (param.control === "list") {
      setOwnKey(data, param.name, []);
    }
  }
  return { data, deferred };
}

export function PrintForm({ detail, stale }: { detail: TemplateDetail; stale?: boolean }) {
  const [value, setValue] = useState<FormValue>(() => ({
    ...initialFieldState(detail.params),
    printer: undefined,
    startSlot: 0,
  }));

  // Selecting a different template reinitialises BOTH values and deferral from the new template's
  // parameters: a name both templates declare must carry nothing across, or template A's value would
  // sit in a disabled control while the render resolved B's default.
  const [renderedTemplateId, setRenderedTemplateId] = useState(detail.id);
  if (renderedTemplateId !== detail.id) {
    setRenderedTemplateId(detail.id);
    setValue((prev) => ({ ...prev, ...initialFieldState(detail.params) }));
  }
  const [fmt, setFmt] = useState<"png" | "pdf">("png");
  const [copies, setCopies] = useState(1);
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { push } = useToast();

  const isLg = useMediaQuery("(min-width: 1024px)");
  const [previewOpen, setPreviewOpen] = useState(false);

  // Printer preselect, derived at render (no effect; #116): default -> sole printer -> none.
  // `value.printer` stores only EXPLICIT user choices ("" = explicit None, an id = explicit pick,
  // undefined = untouched -> use the preselect), so a printers refetch never clobbers a choice.
  const { data: printers } = usePrinters();
  const { data: settings } = useSettings();
  const defaultPrinterId = settings?.default_printer_id?.value;
  const preselect = useMemo(() => {
    const all = printers ?? [];
    return all.find((p) => p.id === defaultPrinterId)?.id ?? (all.length === 1 ? all[0].id : undefined);
  }, [printers, defaultPrinterId]);
  const effectivePrinter = value.printer === undefined ? preselect : value.printer || undefined;

  const showSummary = (summary: BatchSummary) => {
    const { succeeded, total, failed } = summary;
    const detailMsg = failed.length ? ` — ${failed[0].error}` : "";
    push({ kind: failed.length ? "error" : "ok", message: `Printed ${succeeded}/${total}${detailMsg}` });
  };

  const isSheet = detail.format.type === "sheet";
  const startSlot = isSheet ? value.startSlot : undefined;
  const submittedData = pruneDataForSubmit(value.data, detail.params, value.deferred);
  const label = { data: submittedData };

  const preview = useLivePreview(
    { templateId: detail.id, format: detail.format.type, data: submittedData, startSlot },
    isLg || previewOpen,
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
          data: submittedData,
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
        <FieldForm detail={detail} value={{ ...value, printer: effectivePrinter }} onChange={setValue} />

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
            disabled={busy || stale}
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

        {/* -bottom-6 and the 1.5rem in the padding cancel the Shell's p-6, which is inside
            the scrollport: at bottom-0 the bar rests 24px above the floor and the form
            scrolls through the strip beneath it (#371, #372). The padding moves out of the
            style prop because only a class can drop it again at lg, where the bar is static
            and its surface background would read 24px taller than the buttons need. */}
        <div
          className="sticky -bottom-6 z-10 -mx-2 flex flex-wrap items-center gap-2 border-t px-2 pt-3 pb-[calc(0.75rem_+_1.5rem_+_env(safe-area-inset-bottom))] lg:static lg:mx-0 lg:gap-3 lg:border-t-0 lg:px-0 lg:pb-3"
          style={{
            background: "var(--surface)",
            borderColor: "var(--border)",
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
            disabled={busy || !effectivePrinter || stale}
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

