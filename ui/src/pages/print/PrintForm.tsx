import { useMemo, useState } from "react";
import { FieldForm, type FormValue } from "./FieldForm";
import { useLivePreview } from "../../lib/livePreview";
import { useMediaQuery } from "../../lib/useMediaQuery";
import { pruneDataForSubmit } from "../../lib/labelInputs";
import { ApiError, printBatch, renderBatch, saveBlob, sentMessage } from "../../api/client";
import { usePrinters, useSettings } from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { PrintSummary, TemplateDetail } from "../../api/types";
import { PreviewPane } from "../../components/PreviewPane";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };

const buttonBase =
  "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

const MIN_COPIES = 1;
const MAX_COPIES = 100;
const clampCopies = (n: number) => Math.max(MIN_COPIES, Math.min(MAX_COPIES, Math.floor(Number.isFinite(n) ? n : 1)));

export function PrintForm({ detail, stale }: { detail: TemplateDetail; stale?: boolean }) {
  const [value, setValue] = useState<FormValue>({ data: {}, printer: undefined, startSlot: 0 });

  // Values belong to the template they were entered for.
  const [renderedTemplateId, setRenderedTemplateId] = useState(detail.id);
  if (renderedTemplateId !== detail.id) {
    setRenderedTemplateId(detail.id);
    setValue((prev) => ({ ...prev, data: {} }));
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

  const showSummary = (summary: PrintSummary, printer: string) => {
    const name = printers?.find((p) => p.id === printer)?.name ?? printer;
    push({ kind: summary.failed.length ? "error" : "ok", message: sentMessage(summary, name) });
  };

  const isSheet = detail.format.type === "sheet";
  const startSlot = isSheet ? value.startSlot : undefined;
  const submittedData = pruneDataForSubmit(value.data, detail.params);
  const label = { data: submittedData };

  const preview = useLivePreview(
    { templateId: detail.id, format: detail.format.type, data: submittedData, startSlot },
    isLg || previewOpen,
  );

  // A refused batch carries each label's own error in its failures; show those, not the outer message.
  const reportError = (err: unknown, fallback: string) => {
    if (err instanceof ApiError && err.code === "BatchInvalid") {
      const failures = (err.details as BatchFailures)?.failures ?? [];
      const message = failures.map((f) => f.message).join("; ") || err.message;
      setFormError(message);
      push({ kind: "error", message });
    } else {
      push({ kind: "error", message: err instanceof Error ? err.message : fallback });
    }
  };

  const onDownload = async () => {
    setFormError(null);
    if (stale) return; // detail is the previous template during a switch (keepPreviousData); do not submit
    setBusy(true);
    try {
      const labels = Array.from({ length: clampCopies(copies) }, () => label);
      const { blob, filename } = await renderBatch(
        isSheet
          ? { template: detail.id, labels, ...(startSlot ? { start_slot: startSlot } : {}) }
          : { template: detail.id, labels, format: fmt },
      );
      saveBlob(blob, filename ?? `${detail.id}.${isSheet ? "pdf" : "zip"}`);
    } catch (err) {
      reportError(err, "Download failed");
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
      const summary = await printBatch({
        template: detail.id,
        printer,
        labels: Array.from({ length: clampCopies(copies) }, () => label),
        ...(startSlot ? { start_slot: startSlot } : {}),
      });
      showSummary(summary, printer);
    } catch (err) {
      reportError(err, "Print failed");
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

