import { useRef, useState } from "react";
import { useTemplates, useTemplate, usePrinters } from "../api/queries";
import { reconcileRowOptions, referencedFields } from "../lib/templateFields";
import {
  MAX_BATCH_LABELS,
  expandedCount,
  resolveLabels,
  sourceRowForExpandedIndex,
  duplicateRow,
  removeRow,
  validateOptionCell,
  newId,
  type LabelGridRow,
} from "../lib/labelGrid";
import { parseCsv } from "../lib/csv";
import { LabelGrid } from "../components/LabelGrid";
import { ApiError, saveBlob, submitBatch } from "../api/client";
import { useToast } from "../app/toast-context";
import type { TemplateDetail } from "../api/types";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };
const buttonBase = "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;

export function Import() {
  const { data: templates } = useTemplates();
  const { data: printers } = usePrinters();
  const { push } = useToast();

  const [templateId, setTemplateId] = useState("");
  const { data: detail } = useTemplate(templateId);

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
      <CsvEditor detail={detail} printers={(printers ?? []).filter((p) => p.enabled)} push={push} />
    </div>
  );
}

function CsvEditor({
  detail,
  printers,
  push,
}: {
  detail?: TemplateDetail;
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
  const [applyValue, setApplyValue] = useState<Record<string, string>>({}); // chosen "Apply to all" value per option
  const [copies, setCopies] = useState(1);
  const [startSlot, setStartSlot] = useState(0);
  const [printer, setPrinter] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const declaredOptions = detail?.options ?? {};
  const declaredNames = Object.keys(declaredOptions);
  const optionNames = declaredNames; // every declared option is an always-present per-row column
  const isSingleValued = (name: string) => (declaredOptions[name]?.length ?? 0) <= 1;
  const isSheet = detail?.format.type === "sheet";

  // Effective per-row options against the CURRENT template: missing options default to first-allowed,
  // values for options the current template no longer declares are dropped. Stored rows keep their raw
  // option map; this view-time reconciliation is what makes a template switch correct without a remount.
  const effectiveOption = (row: LabelGridRow): Record<string, string> => reconcileRowOptions(row.option, declaredOptions);

  // Fields required for a row depend on THAT row's effective options (a CSV option.<name> column can
  // vary per row and gate different containers), so this is computed per row. With no template, no fields.
  const requiredForRow = (row: LabelGridRow): string[] => (detail ? referencedFields(detail.layout, effectiveOption(row)) : []);
  // Grid columns: CSV columns plus any required field (across all row variants) the CSV omits.
  const requiredUnion = new Set<string>();
  for (const row of rows) for (const f of requiredForRow(row)) requiredUnion.add(f);
  const baseRequired = !detail ? [] : rows.length ? [...requiredUnion] : referencedFields(detail.layout, reconcileRowOptions({}, declaredOptions));
  const displayedFields = [...csvFields, ...baseRequired.filter((f) => !csvFields.includes(f))];

  // One validation function, used both for render (viewRows) and as the run() submit guard, so a value
  // committed on blur right before a click cannot be submitted while the button is still showing enabled.
  const validateRow = (row: LabelGridRow): LabelGridRow["validation"] => {
    const field: Record<string, string> = {};
    for (const f of requiredForRow(row)) if ((row.data[f] ?? "").length === 0) field[f] = "required";
    const eff = effectiveOption(row);
    const option: Record<string, string> = {};
    for (const name of optionNames) {
      const err = validateOptionCell(eff[name] ?? "", declaredOptions[name]);
      if (err) option[name] = err;
    }
    const v: LabelGridRow["validation"] = {};
    if (Object.keys(field).length) v.field = field;
    if (Object.keys(option).length) v.option = option;
    return v;
  };
  const rowInvalid = (row: LabelGridRow): boolean => {
    const v = validateRow(row);
    return !!v.field || !!v.option;
  };
  // Validation is derived fresh each render (never stored), so it cannot go stale when options change.
  // Options are reconciled for display so the grid shows defaults; an edit then commits the reconciled value.
  const viewRows: LabelGridRow[] = rows.map((row) => ({ ...row, option: effectiveOption(row), validation: validateRow(row) }));
  const hasErrors = viewRows.some(rowInvalid);

  const total = expandedCount(rows.length, copies);
  const overCap = total > MAX_BATCH_LABELS;

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
    // Only flag "ignored" option columns once a template is chosen; with no template, a CSV option.<name>
    // may match a template selected later, so keep its raw value and do not warn yet.
    const undeclared = detail ? parsed.optionColumns.filter((n) => !declaredNames.includes(n)) : [];
    setCsvFields(parsed.fields);
    setIssues([...parsed.issues, ...undeclared.map((n) => `Column option.${n} is not a declared option and is ignored.`)]);
    const built = parsed.rows.map<LabelGridRow>((r) => ({
      id: newId(),
      origin: "csv",
      data: { ...r.data },
      option: { ...r.option }, // raw CSV option values; effectiveOption reconciles them against the current template at render
      validation: {},
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
    // Imperative submit guards (defense in depth; the buttons are also disabled for these, but the
    // disabled state lags a blur-commit by one render). Validate the live snapshot and the cap/printer.
    const snapshot = rowsRef.current;
    if (snapshot.length === 0) return;
    if (snapshot.some(rowInvalid)) {
      setFormError("Fix the highlighted rows before running.");
      return;
    }
    if (expandedCount(snapshot.length, copies) > MAX_BATCH_LABELS) {
      setFormError(`Too many labels (over the ${MAX_BATCH_LABELS} limit).`);
      return;
    }
    if (mode === "print" && !printer) {
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
      // Reconcile each row's options against the current template before submit (ids unchanged), then
      // resolve with no global overlay (per-row options already carry the effective value).
      const submitRows = rowsRef.current.map((r) => ({ ...r, option: effectiveOption(r) }));
      const labels = resolveLabels(submitRows, {}, submittedCopies);
      const r = await submitBatch({
        template: detail.id,
        labels,
        mode,
        ...(mode === "print" ? { printer } : {}),
        ...(isSheet && startSlot ? { start_slot: startSlot } : {}),
      });
      if (r.kind === "download") {
        // Sheet downloads are a composed PDF; single-template batches are a ZIP.
        saveBlob(r.blob, r.filename ?? `${detail.id}.${isSheet ? "pdf" : "zip"}`);
        push({ kind: "ok", message: `Downloaded ${labels.length} labels` });
      } else {
        const { succeeded, total: t, failed } = r.summary;
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
        push({ kind: failed.length ? "error" : "ok", message: `Printed ${succeeded}/${t}` });
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

      {issues.length > 0 && (
        <ul className="text-sm" style={{ color: "var(--bad)" }}>
          {issues.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}

      {rows.length > 0 && (
        <>
          {detail && optionNames.filter((n) => !isSingleValued(n)).length > 0 && (
            <div className="flex flex-wrap gap-3">
              {optionNames.filter((n) => !isSingleValued(n)).map((name) => (
                <div key={name} className="flex items-end gap-2">
                  <label className="flex flex-col gap-1">
                    <span className="text-sm font-medium">{name} (all rows)</span>
                    <select
                      aria-label={`set all ${name}`}
                      value={declaredOptions[name].includes(applyValue[name]) ? applyValue[name] : declaredOptions[name][0] ?? ""}
                      disabled={busy}
                      onChange={(e) => setApplyValue({ ...applyValue, [name]: e.target.value })}
                      className={inputClass}
                      style={inputStyle}
                    >
                      {declaredOptions[name].map((v) => (<option key={v} value={v}>{v}</option>))}
                    </select>
                  </label>
                  <button
                    type="button"
                    aria-label={`apply ${name} to all rows`}
                    disabled={busy}
                    onClick={() => {
                      const v = declaredOptions[name].includes(applyValue[name]) ? applyValue[name] : declaredOptions[name][0] ?? "";
                      commitRows(rowsRef.current.map((r) => ({ ...r, option: { ...effectiveOption(r), [name]: v }, annotation: undefined })));
                      setFormError(null);
                    }}
                    className={`${buttonBase} border`}
                    style={{ borderColor: "var(--border)", color: "var(--ink)" }}
                  >
                    Apply to all
                  </button>
                </div>
              ))}
            </div>
          )}

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

          <LabelGrid
            rows={viewRows}
            fields={displayedFields}
            optionNames={optionNames}
            optionValues={declaredOptions}
            onRowsChange={(next, { indexes }) => {
              // viewRows carries derived validation (and rows may carry a prior run's annotation); store
              // only canonical data: drop validation everywhere and clear annotation on the edited rows.
              const dirty = new Set(indexes);
              commitRows(next.map((r, i) => ({ ...r, validation: {}, annotation: dirty.has(i) ? undefined : r.annotation })));
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
          />

          {detail && (
          <div className="sticky bottom-0 flex flex-wrap items-center gap-3 border-t py-3" style={{ background: "var(--bg)", borderColor: "var(--border)" }}>
            <button
              type="button"
              onClick={() => run("print")}
              disabled={busy || overCap || hasErrors || !printer}
              className={buttonBase}
              style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
            >
              Print
            </button>
            <button
              type="button"
              onClick={() => run("download")}
              disabled={busy || overCap || hasErrors}
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
