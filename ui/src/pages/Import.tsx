import { useMemo, useRef, useState } from "react";
import { useTemplates, useTemplate, usePrinters } from "../api/queries";
import {
  MAX_BATCH_LABELS,
  expandedCount,
  sourceRowForExpandedIndex,
  duplicateRow,
  removeRow,
  resolveLabels,
  sheetPreviewBlock,
  newId,
  type LabelGridRow,
} from "../lib/labelGrid";
import { parseCsv } from "../lib/csv";
import { LabelGrid } from "../components/LabelGrid";
import { PreviewPane } from "../components/PreviewPane";
import { useRowPreview } from "../lib/rowPreview";
import { useSheetPreview } from "../lib/sheetPreview";
import { pruneDataForSubmit } from "../lib/labelInputs";
import { ApiError, printBatch, renderBatch, saveBlob, sentMessage } from "../api/client";
import { useToast } from "../app/toast-context";
import { EmptyTemplates } from "../components/EmptyTemplates";
import type { TemplateDetail } from "../api/types";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };
const buttonBase = "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;

export function Import() {
  const { data: templates, isError: templatesFailed } = useTemplates();
  const { data: printers } = usePrinters();
  const { push } = useToast();

  const [templateId, setTemplateId] = useState("");
  const { data: detail, isPlaceholderData } = useTemplate(templateId);

  if (templatesFailed) {
    return (
      <div className="flex flex-col gap-4">
        <h1 className="text-2xl font-semibold">Import</h1>
        <p style={{ color: "var(--bad)" }}>Couldn&apos;t load templates.</p>
      </div>
    );
  }

  if (templates && (templates.templates ?? []).length === 0) {
    return (
      <div className="flex flex-col gap-4">
        <h1 className="text-2xl font-semibold">Import</h1>
        <EmptyTemplates context="Importing a CSV needs a template to render each row into." />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Import</h1>
      <label className="flex flex-col gap-1 max-w-sm">
        <span className="text-sm font-medium">Template</span>
        <select aria-label="template" value={templateId} onChange={(e) => setTemplateId(e.target.value)} className={inputClass} style={inputStyle}>
          <option value="">choose a template</option>
          {(templates?.templates ?? []).map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </select>
      </label>
      <CsvEditor detail={detail} stale={isPlaceholderData} printers={printers ?? []} push={push} />
    </div>
  );
}

function CsvEditor({
  detail,
  stale,
  printers,
  push,
}: {
  detail?: TemplateDetail;
  stale?: boolean;
  printers: { id: string; name: string }[];
  push: (t: { kind: "ok" | "error"; message: string }) => void;
}) {
  const [text, setText] = useState("");
  const [loadedSource, setLoadedSource] = useState(""); // last successfully parsed CSV text (for Reset)
  const [rows, setRows] = useState<LabelGridRow[]>([]);
  // A ref mirrors `rows` so event handlers (notably run(), which fires right after an edit's blur-commit)
  // read the latest rows synchronously, not a stale render closure. Every mutation goes through commitRows.
  const rowsRef = useRef(rows);
  const commitRows = (next: LabelGridRow[]) => {
    rowsRef.current = next;
    setRows(next);
  };
  const [csvFields, setCsvFields] = useState<string[]>([]);
  const [issues, setIssues] = useState<string[]>([]);
  const [copies, setCopies] = useState(1);
  const [startSlot, setStartSlot] = useState(0);
  const [printer, setPrinter] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [selectedRowId, setSelectedRowId] = useState<string | undefined>(undefined);

  const isSheet = detail?.format.type === "sheet";

  const params = useMemo(() => detail?.params ?? [], [detail]);
  const templateFields = useMemo(() => params.map((p) => p.name), [params]);
  const listNames = useMemo(
    () => new Set(params.filter((p) => p.control === "list").map((p) => p.name)),
    [params],
  );

  const displayedFields = useMemo(() => {
    const set = new Set(csvFields);
    for (const f of templateFields) set.add(f);
    return [...set].filter((f) => !listNames.has(f));
  }, [csvFields, templateFields, listNames]);

  const cellInput = (field: string) => (detail ? params.find((p) => p.name === field) : { name: field, control: "text" as const });

  // The first row is previewed when none is selected or the selected one was removed.
  const resolvedSelectedId = rows.some((r) => r.id === selectedRowId) ? selectedRowId : rows[0]?.id;

  const dataFor = (r: LabelGridRow) => pruneDataForSubmit(r.data, params);

  // Build the resolved label for the selected row using the same resolution the submit path uses.
  const selRow = rows.find((r) => r.id === resolvedSelectedId);
  const previewData = detail && selRow ? dataFor(selRow) : undefined;
  const previewLabel = previewData ? { data: previewData } : undefined;

  const total = expandedCount(rows.length, copies);
  const overCap = total > MAX_BATCH_LABELS;

  const blocked = sheetPreviewBlock(total);

  const sheetPreview = useSheetPreview(
    { templateId: detail?.id ?? "", labels: resolveLabels(rows, copies, dataFor), startSlot },
    isSheet && rows.length > 0 && !blocked,
  );
  const rowPreview = useRowPreview({
    templateId: detail?.id ?? "",
    label: isSheet ? undefined : detail ? previewLabel : undefined,
  });

  const preview = isSheet ? (blocked ? { loading: false, blocked } : sheetPreview) : rowPreview;

  const ignoredNotices: string[] = [];

  const clearGrid = () => {
    commitRows([]);
    setCsvFields([]);
    setLoadedSource("");
  };

  const loadFrom = (raw: string) => {
    setFormError(null); // a fresh load clears any prior submit error
    const parsed = parseCsv(raw);
    // A malformed CSV (papaparse error) must not be submittable: surface the issues and load nothing.
    if (parsed.fatal) {
      setIssues(parsed.issues);
      clearGrid();
      return;
    }
    // The grid is non-virtualized and a batch caps at 500 labels, so reject CSVs over the row cap up front
    // (a larger file could never submit, and rendering thousands of rows would freeze the UI).
    if (parsed.rows.length > MAX_BATCH_LABELS) {
      setIssues([`CSV has ${parsed.rows.length} rows; the limit is ${MAX_BATCH_LABELS}.`]);
      clearGrid();
      return;
    }
    setCsvFields(parsed.fields);
    setIssues(parsed.issues);
    const built = parsed.rows.map<LabelGridRow>((r) => ({
      id: newId(),
      origin: "csv",
      data: { ...r.data },
    }));
    commitRows(built);
    setLoadedSource(raw);
  };

  // Reset reloads the originally parsed CSV: removed rows return in their original order, edits and
  // duplicates are discarded, and copies returns to 1. Reloading is deterministic and avoids the
  // index-tracking bugs of trying to splice removed rows back into a mutated list.
  const onReset = () => {
    loadFrom(loadedSource);
    setCopies(1);
  };

  const run = async (mode: "download" | "print") => {
    setFormError(null);
    if (!detail) return; // no template selected: nothing to render/submit
    if (stale) return; // detail is the previous template during a switch (keepPreviousData); do not submit
    // Imperative submit guards (defense in depth; the buttons are also disabled for these, but the
    // disabled state lags a blur-commit by one render). Check the live snapshot against the printer.
    const snapshot = rowsRef.current;
    if (snapshot.length === 0) return;
    const printTo = mode === "print" ? printer : undefined;
    if (mode === "print" && !printTo) {
      setFormError("Select a printer to print.");
      return;
    }
    setBusy(true);
    // Clear stale annotations from a previous run so a later validation failure cannot leave old results visible.
    commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
    // Snapshot the submitted rows' ids and the copies used, so a failure index maps to the right ROW
    // even if the grid mutates in flight (annotate by id, not array index). Read rowsRef so a just-committed
    // cell edit (blur fires before this click handler) is included.
    const submittedIds = rowsRef.current.map((r) => r.id);
    const submittedCopies = copies;
    const idForExpandedIndex = (index: number): string | undefined => submittedIds[sourceRowForExpandedIndex(index, submittedCopies)];
    try {
      const labels = resolveLabels(rowsRef.current, submittedCopies, dataFor);
      const slot = isSheet && startSlot ? { start_slot: startSlot } : {};
      if (!printTo) {
        const { blob, filename } = await renderBatch({ template: detail.id, labels, ...slot });
        // Sheet downloads are a composed PDF; single-template batches are a ZIP.
        saveBlob(blob, filename ?? `${detail.id}.${isSheet ? "pdf" : "zip"}`);
        push({ kind: "ok", message: `Downloaded ${labels.length} labels` });
      } else {
        const summary = await printBatch({ template: detail.id, labels, printer: printTo, ...slot });
        const { failed } = summary;
        const failById = new Map<string, string>();
        for (const f of failed) {
          const id = idForExpandedIndex(f.index);
          if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.error}` : f.error);
        }
        // All submitted rows that still exist are annotated ok unless they failed.
        const submitted = new Set(submittedIds);
        commitRows(
          rowsRef.current.map((row) =>
            submitted.has(row.id)
              ? { ...row, annotation: failById.has(row.id) ? { status: "failed", message: failById.get(row.id) } : { status: "ok" } }
              : row,
          ),
        );
        const name = printers?.find((p) => p.id === printTo)?.name ?? printTo;
        push({ kind: failed.length ? "error" : "ok", message: sentMessage(summary, name) });
      }
    } catch (err) {
      if (err instanceof ApiError && err.code === "BatchInvalid") {
        const failures = (err.details as BatchFailures)?.failures ?? [];
        const failById = new Map<string, string>();
        for (const f of failures) {
          const id = idForExpandedIndex(f.index);
          if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.message}` : f.message);
        }
        commitRows(rowsRef.current.map((row) => (failById.has(row.id) ? { ...row, annotation: { status: "failed", message: failById.get(row.id) } } : row)));
        const message = failures.map((f) => f.message).join("; ") || err.message;
        setFormError(message);
        push({ kind: "error", message });
      } else {
        const message = err instanceof Error ? err.message : "Batch failed";
        push({ kind: "error", message });
      }
    } finally {
      setBusy(false);
    }
  };

  const positions = detail?.format.type === "sheet" ? detail.format.positions.length : 0;

  return (
    <div className="flex flex-col gap-4">
      <div
        aria-label="csv dropzone"
        onDragOver={(e) => e.preventDefault()}
        onDrop={async (e) => {
          e.preventDefault();
          const file = e.dataTransfer.files?.[0];
          if (file) {
            const content = await file.text();
            setText(content);
            loadFrom(content);
          }
        }}
        className="flex max-w-sm flex-col gap-1 rounded-md border border-dashed p-4"
        style={{ borderColor: "var(--border)" }}
      >
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">CSV file</span>
          <input
            type="file"
            accept=".csv,text/csv"
            aria-label="csv file"
            className="text-sm"
            disabled={busy}
            onChange={async (e) => {
              const file = e.target.files?.[0];
              if (file) {
                const content = await file.text();
                setText(content);
                loadFrom(content);
              }
            }}
          />
        </label>
        <span className="text-xs" style={{ color: "var(--muted)" }}>
          or drop a CSV file here
        </span>
      </div>
      <label className="flex flex-col gap-1">
        <span className="text-sm font-medium">Paste CSV</span>
        <textarea aria-label="paste CSV" value={text} onChange={(e) => setText(e.target.value)} rows={4} className={inputClass} style={inputStyle} />
      </label>
      <div>
        <button type="button" onClick={() => loadFrom(text)} disabled={busy} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
          Load CSV
        </button>
      </div>

      {(issues.length > 0 || ignoredNotices.length > 0) && (
        <ul className="text-sm" style={{ color: "var(--bad)" }}>
          {[...issues, ...ignoredNotices].map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}

      {displayedFields.length > 0 && (
        <LabelGrid
          rows={rows}
          fields={displayedFields}
          cellInput={cellInput}
          onRowsChange={(next, { indexes }) => {
            // Rows may carry a prior run's annotation; clear it on the edited rows.
            const dirty = new Set(indexes);
            commitRows(next.map((r, i) => ({ ...r, annotation: dirty.has(i) ? undefined : r.annotation })));
            setFormError(null); // editing invalidates a prior submit error
          }}
          onDuplicate={(id) => {
            // A structural change invalidates the prior run's per-row results, so clear annotations.
            commitRows(duplicateRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined })));
            setFormError(null);
          }}
          onRemove={(id) => {
            commitRows(removeRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined })));
            setFormError(null);
          }}
          disabled={busy}
          selectedRowId={isSheet ? undefined : resolvedSelectedId}
          onSelectRow={isSheet ? undefined : setSelectedRowId}
        />
      )}

      {rows.length > 0 && (
        <>
          {detail && (
          <div className="flex flex-wrap items-end gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-sm font-medium">Copies</span>
              <input
                type="number"
                min={1}
                aria-label="copies"
                value={copies}
                disabled={busy}
                onChange={(e) => {
                  setCopies(Math.max(1, Math.floor(Number(e.target.value) || 1)));
                  commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
                  setFormError(null);
                }}
                className={inputClass}
                style={inputStyle}
              />
            </label>
            {isSheet && (
              <label className="flex flex-col gap-1">
                <span className="text-sm font-medium">Start slot</span>
                <input
                  type="number"
                  min={0}
                  max={Math.max(0, positions - 1)}
                  aria-label="start slot"
                  value={startSlot}
                  disabled={busy}
                  onChange={(e) => {
                    setStartSlot(Math.max(0, Math.min(positions - 1, Math.floor(Number(e.target.value) || 0))));
                    commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
                    setFormError(null);
                  }}
                  className={inputClass}
                  style={inputStyle}
                />
              </label>
            )}
            <label className="flex flex-col gap-1">
              <span className="text-sm font-medium">Printer</span>
              <select
                aria-label="printer"
                value={printer ?? ""}
                disabled={busy}
                onChange={(e) => {
                  setPrinter(e.target.value || undefined);
                  setFormError(null);
                }}
                className={inputClass}
                style={inputStyle}
              >
                <option value="">none (download only)</option>
                {printers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <button type="button" onClick={onReset} disabled={busy} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
              ↺ Reset
            </button>
          </div>
          )}

          {detail && (
          <PreviewPane name={detail.name} format={isSheet ? "sheet" : "single"} preview={preview} />
          )}

          {/* An action bar over the preview has to be opaque and reach the scrollport floor.
              --bg is defined nowhere, so the background computed to transparent, and at
              bottom-0 the bar stopped 24px short on the Shell's p-6, leaving the preview
              visible through and beneath it. The 1.5rem in the padding is that p-6 again. The
              inset is the Print bar's spelling of the same 2.25rem and is inert until
              index.html asks for viewport-fit=cover, without which env() is 0 (#374).
              z-10 matches the Print page's bar. */}
          {detail && (
          <div className="sticky -bottom-6 z-10 flex flex-wrap items-center gap-3 border-t pt-3 pb-[calc(0.75rem_+_1.5rem_+_env(safe-area-inset-bottom))]" style={{ background: "var(--paper)", borderColor: "var(--border)" }}>
            <button
              type="button"
              onClick={() => run("print")}
              disabled={busy || !printer || stale}
              className={buttonBase}
              style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
            >
              Print
            </button>
            <button
              type="button"
              onClick={() => run("download")}
              disabled={busy || stale}
              className={`${buttonBase} border`}
              style={{ borderColor: "var(--border)", color: "var(--ink)" }}
            >
              Download
            </button>
            <span className="text-sm" style={{ color: "var(--muted)" }}>{total} labels</span>
            {overCap && <span style={{ color: "var(--bad)" }}>over the {MAX_BATCH_LABELS}-label limit</span>}
            {formError && <span style={{ color: "var(--bad)" }}>{formError}</span>}
          </div>
          )}
        </>
      )}
    </div>
  );
}
