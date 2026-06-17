import { useMemo, useRef, useState } from "react";
import { useConnections, useConnectorSchema, materializeConnection, type ConnectorSchema, type RowRef } from "../api/connectors";
import { ConnectorBrowser } from "./connect/ConnectorBrowser";
import { useTemplates, useTemplate, usePrinters } from "../api/queries";
import { referencedFields, defaultOptions } from "../lib/templateFields";
import { defaultMapping, mappedConnectorKeys, rowsFromMaterialized, type FieldMapping } from "../lib/connectorRows";
import {
  MAX_BATCH_LABELS, expandedCount, resolveLabels, sourceRowForExpandedIndex,
  duplicateRow, removeRow, type LabelGridRow,
} from "../lib/labelGrid";
import { LabelGrid } from "../components/LabelGrid";
import { ApiError, saveBlob, submitBatch } from "../api/client";
import { useToast } from "../app/toast-context";
import type { TemplateDetail } from "../api/types";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };
const buttonBase = "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const MATERIALIZE_CAP = 200; // backend /materialize rejects more than this in one call (400 BudgetExceeded)

export function Connect() {
  const { data: connections } = useConnections();
  const { data: templates } = useTemplates();
  const { data: printers } = usePrinters();

  const [connectionId, setConnectionId] = useState("");
  const { data: schema } = useConnectorSchema(connectionId);
  const [templateId, setTemplateId] = useState("");
  const { data: detail } = useTemplate(templateId);

  const [selected, setSelected] = useState<RowRef[]>([]);
  const conn = (connections ?? []).find((c) => c.id === connectionId);

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Connect</h1>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Connection</span>
          <select aria-label="connection" value={connectionId} onChange={(e) => { setConnectionId(e.target.value); setSelected([]); }} className={inputClass} style={inputStyle}>
            <option value="">choose a connection</option>
            {(connections ?? []).filter((c) => c.enabled).map((c) => (<option key={c.id} value={c.id}>{c.name}</option>))}
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Template</span>
          <select aria-label="template" value={templateId} onChange={(e) => setTemplateId(e.target.value)} className={inputClass} style={inputStyle}>
            <option value="">choose a template</option>
            {(templates?.templates ?? []).map((t) => (<option key={t.id} value={t.id}>{t.name}</option>))}
          </select>
        </label>
      </div>

      {connectionId && schema && (
        <ConnectorBrowser key={connectionId} connectionId={connectionId} schema={schema} selected={selected} onSelectedChange={setSelected} />
      )}

      {connectionId && schema && detail && conn && (
        <Composer
          key={`${connectionId}:${detail.id}`}
          connectionId={connectionId}
          connectorId={conn.connector}
          schema={schema}
          detail={detail}
          selected={selected}
          printers={(printers ?? []).filter((p) => p.enabled)}
        />
      )}
    </div>
  );
}

function Composer({
  connectionId, connectorId, schema, detail, selected, printers,
}: {
  connectionId: string;
  connectorId: string;
  schema: ConnectorSchema;
  detail: TemplateDetail;
  selected: RowRef[];
  printers: { id: string; name: string }[];
}) {
  const { push } = useToast();
  const connectorKeys = useMemo(() => [...new Set(schema.resources.flatMap((r) => r.columns.map((c) => c.key)))], [schema]);
  const templateFields = useMemo(() => referencedFields(detail.layout, {}), [detail]);
  const [mapping, setMapping] = useState<FieldMapping>(() => defaultMapping(templateFields, connectorKeys));

  const [rows, setRows] = useState<LabelGridRow[]>([]);
  const rowsRef = useRef(rows);
  const commitRows = (next: LabelGridRow[]) => { rowsRef.current = next; setRows(next); };

  const [manualOptions, setManualOptions] = useState<Record<string, string>>(() => defaultOptions(detail.options));
  const [copies, setCopies] = useState(1);
  const [startSlot, setStartSlot] = useState(0);
  const [printer, setPrinter] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const declaredOptions = detail.options ?? {};
  const declaredNames = Object.keys(declaredOptions);
  const isSheet = detail.format.type === "sheet";
  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;

  const requiredForRow = (row: LabelGridRow): string[] => referencedFields(detail.layout, { ...manualOptions, ...row.option });
  const requiredUnion = new Set<string>();
  for (const row of rows) for (const f of requiredForRow(row)) requiredUnion.add(f);
  const displayedFields = rows.length ? [...requiredUnion] : referencedFields(detail.layout, manualOptions);

  const validateRow = (row: LabelGridRow): LabelGridRow["validation"] => {
    const field: Record<string, string> = {};
    for (const f of requiredForRow(row)) if ((row.data[f] ?? "").length === 0) field[f] = "required";
    return Object.keys(field).length ? { field } : {};
  };
  const rowInvalid = (row: LabelGridRow): boolean => !!validateRow(row).field;
  const viewRows = rows.map((row) => ({ ...row, validation: validateRow(row) }));
  const hasErrors = viewRows.some(rowInvalid);
  const total = expandedCount(rows.length, copies);
  const overCap = total > MAX_BATCH_LABELS;

  const addRows = async () => {
    if (selected.length === 0) return;
    setFormError(null);
    if (selected.length > MATERIALIZE_CAP) { setFormError(`Select at most ${MATERIALIZE_CAP} rows at a time.`); return; }
    if (rowsRef.current.length + selected.length > MAX_BATCH_LABELS) { setFormError(`That would exceed the ${MAX_BATCH_LABELS}-row limit.`); return; }
    setBusy(true);
    try {
      const fields = mappedConnectorKeys(mapping);
      const materialized = await materializeConnection(connectionId, { rows: selected, fields, expansion: "as_listed" });
      const built = rowsFromMaterialized(materialized, mapping, connectorId, connectionId);
      commitRows([...rowsRef.current, ...built]);
      push({ kind: "ok", message: `Added ${built.length} rows` });
    } catch (err) {
      const message = err instanceof Error ? err.message : "Materialize failed";
      setFormError(message); push({ kind: "error", message });
    } finally {
      setBusy(false);
    }
  };

  const run = async (mode: "download" | "print") => {
    setFormError(null);
    const snapshot = rowsRef.current;
    if (snapshot.length === 0) return;
    if (snapshot.some(rowInvalid)) { setFormError("Fix the highlighted rows before running."); return; }
    if (expandedCount(snapshot.length, copies) > MAX_BATCH_LABELS) { setFormError(`Too many labels (over the ${MAX_BATCH_LABELS} limit).`); return; }
    if (mode === "print" && !printer) { setFormError("Select a printer to print."); return; }
    setBusy(true);
    commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
    const submittedIds = rowsRef.current.map((r) => r.id);
    const submittedCopies = copies;
    const idForExpandedIndex = (index: number): string | undefined => submittedIds[sourceRowForExpandedIndex(index, submittedCopies)];
    try {
      const labels = resolveLabels(rowsRef.current, manualOptions, submittedCopies);
      const r = await submitBatch({
        template: detail.id, labels, mode,
        ...(mode === "print" ? { printer } : {}),
        ...(isSheet && startSlot ? { start_slot: startSlot } : {}),
      });
      if (r.kind === "download") {
        saveBlob(r.blob, r.filename ?? `${detail.id}.${isSheet ? "pdf" : "zip"}`);
        push({ kind: "ok", message: `Downloaded ${labels.length} labels` });
      } else {
        const { succeeded, total: t, failed } = r.summary;
        const failById = new Map<string, string>();
        for (const f of failed) { const id = idForExpandedIndex(f.index); if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.error}` : f.error); }
        const submitted = new Set(submittedIds);
        commitRows(rowsRef.current.map((row) =>
          submitted.has(row.id)
            ? { ...row, annotation: failById.has(row.id) ? { status: "failed", message: failById.get(row.id) } : { status: "ok" } }
            : row));
        push({ kind: failed.length ? "error" : "ok", message: `Printed ${succeeded}/${t}` });
      }
    } catch (err) {
      if (err instanceof ApiError && err.code === "BatchInvalid") {
        const failures = (err.details as BatchFailures)?.failures ?? [];
        const failById = new Map<string, string>();
        for (const f of failures) { const id = idForExpandedIndex(f.index); if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.message}` : f.message); }
        commitRows(rowsRef.current.map((row) => (failById.has(row.id) ? { ...row, annotation: { status: "failed", message: failById.get(row.id) } } : row)));
        const message = failures.map((f) => f.message).join("; ") || err.message;
        setFormError(message); push({ kind: "error", message });
      } else {
        const message = err instanceof Error ? err.message : "Batch failed";
        push({ kind: "error", message });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-col gap-2 rounded-md border p-4" style={{ borderColor: "var(--border)" }}>
        <h2 className="text-sm font-semibold">Field mapping</h2>
        <div className="flex flex-wrap gap-3">
          {templateFields.map((field) => (
            <label key={field} className="flex flex-col gap-1">
              <span className="text-xs" style={{ color: "var(--muted)" }}>{field}</span>
              <select aria-label={`map ${field}`} value={mapping[field] ?? ""} onChange={(e) => setMapping({ ...mapping, [field]: e.target.value })} className={inputClass} style={inputStyle}>
                <option value="">(blank)</option>
                {connectorKeys.map((k) => (<option key={k} value={k}>{k}</option>))}
              </select>
            </label>
          ))}
        </div>
        <div>
          <button type="button" onClick={addRows} disabled={busy || selected.length === 0} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
            Add {selected.length} {selected.length === 1 ? "row" : "rows"}
          </button>
        </div>
      </section>

      {rows.length > 0 && (
        <>
          {declaredNames.length > 0 && (
            <div className="flex flex-wrap gap-3">
              {declaredNames.map((name) => (
                <label key={name} className="flex flex-col gap-1">
                  <span className="text-sm font-medium">{name}</span>
                  <select aria-label={name} value={manualOptions[name] ?? declaredOptions[name][0] ?? ""} disabled={busy}
                    onChange={(e) => { setManualOptions({ ...manualOptions, [name]: e.target.value }); commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
                    className={inputClass} style={inputStyle}>
                    {declaredOptions[name].map((v) => (<option key={v} value={v}>{v}</option>))}
                  </select>
                </label>
              ))}
            </div>
          )}

          <div className="flex flex-wrap items-end gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-sm font-medium">Copies</span>
              <input type="number" min={1} aria-label="copies" value={copies} disabled={busy}
                onChange={(e) => { setCopies(Math.max(1, Math.floor(Number(e.target.value) || 1))); commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
                className={inputClass} style={inputStyle} />
            </label>
            {isSheet && (
              <label className="flex flex-col gap-1">
                <span className="text-sm font-medium">Start slot</span>
                <input type="number" min={0} max={Math.max(0, positions - 1)} aria-label="start slot" value={startSlot} disabled={busy}
                  onChange={(e) => { setStartSlot(Math.max(0, Math.min(positions - 1, Math.floor(Number(e.target.value) || 0)))); commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
                  className={inputClass} style={inputStyle} />
              </label>
            )}
            <label className="flex flex-col gap-1">
              <span className="text-sm font-medium">Printer</span>
              <select aria-label="printer" value={printer ?? ""} disabled={busy} onChange={(e) => { setPrinter(e.target.value || undefined); setFormError(null); }} className={inputClass} style={inputStyle}>
                <option value="">none (download only)</option>
                {printers.map((p) => (<option key={p.id} value={p.id}>{p.name}</option>))}
              </select>
            </label>
            <span className="text-sm" style={{ color: "var(--muted)" }}>{total} labels</span>
          </div>

          {overCap && <p style={{ color: "var(--bad)" }}>{total} labels is over the {MAX_BATCH_LABELS}-label limit. Reduce rows or copies.</p>}
          {formError && <p style={{ color: "var(--bad)" }}>{formError}</p>}

          <LabelGrid
            rows={viewRows}
            fields={displayedFields}
            optionNames={[]}
            optionValues={declaredOptions}
            onRowsChange={(next, { indexes }) => {
              const dirty = new Set(indexes);
              commitRows(next.map((r, i) => ({ ...r, validation: {}, annotation: dirty.has(i) ? undefined : r.annotation })));
              setFormError(null);
            }}
            onDuplicate={(id) => { commitRows(duplicateRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
            onRemove={(id) => { commitRows(removeRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
            disabled={busy}
          />

          <div className="flex gap-3">
            <button type="button" onClick={() => run("print")} disabled={busy || overCap || hasErrors || !printer} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}>Print</button>
            <button type="button" onClick={() => run("download")} disabled={busy || overCap || hasErrors} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Download</button>
          </div>
        </>
      )}
    </div>
  );
}
