import { useMemo, useRef, useState } from "react";
import { useConnections, useConnectorSchema, materializeConnection, type ConnectorSchema, type SelectedRow } from "../api/connectors";
import { ConnectorBrowser } from "./connect/ConnectorBrowser";
import { useTemplates, useTemplate, usePrinters, useSettings } from "../api/queries";
import { EmptyTemplates } from "../components/EmptyTemplates";
import { datetimeCellError } from "../lib/templateFields";
import { defaultMapping, mappedConnectorKeys, rowsFromMaterialized, type FieldMapping } from "../lib/connectorRows";
import {
  MAX_BATCH_LABELS, expandedCount, sourceRowForExpandedIndex,
  duplicateRow, removeRow, type LabelGridRow,
} from "../lib/labelGrid";
import { LabelGrid } from "../components/LabelGrid";
import { PreviewPane } from "../components/PreviewPane";
import { useRowPreview } from "../lib/rowPreview";
import { useBatchRowInputs, pruneDataForSubmit } from "../lib/labelInputs";
import { ApiError, saveBlob, submitBatch } from "../api/client";
import { useToast } from "../app/toast-context";
import type { TemplateDetail, InputSpec } from "../api/types";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };
const buttonBase = "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const MATERIALIZE_CAP = 200; // backend /materialize rejects more than this in one call (400 BudgetExceeded)

export function Connect() {
  const { data: connections, isError: connectionsFailed } = useConnections();
  const { data: settings, isError: settingsFailed } = useSettings();
  const { data: templates, isError: templatesFailed } = useTemplates();
  const { data: printers } = usePrinters();

  const [selectedConnectionId, setSelectedConnectionId] = useState<string | null>(null);
  const [latchedConnectionId, setLatchedConnectionId] = useState<string | null>(null);

  if (latchedConnectionId === null) {
    if (connectionsFailed) {
      setLatchedConnectionId("");
    } else if (connections !== undefined && (settings !== undefined || settingsFailed)) {
      const defaultId =
        typeof settings?.default_connection_id?.value === "string"
          ? settings.default_connection_id.value
          : null;
      const defaultConn = defaultId
        ? connections.find((c) => c.id === defaultId && c.enabled)
        : null;
      const fallbackConn = connections.find((c) => c.enabled);
      const resolved = (defaultConn ?? fallbackConn)?.id ?? "";
      setLatchedConnectionId(resolved);
    }
  }

  const connectionId =
    selectedConnectionId !== null ? selectedConnectionId : (latchedConnectionId ?? "");
  const { data: schema } = useConnectorSchema(connectionId);
  const [templateId, setTemplateId] = useState("");
  const { data: detail, isPlaceholderData } = useTemplate(templateId);

  const [selected, setSelected] = useState<SelectedRow[]>([]);
  const conn = (connections ?? []).find((c) => c.id === connectionId);

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Connect</h1>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Connection</span>
          <select aria-label="connection" value={connectionId} onChange={(e) => { setSelectedConnectionId(e.target.value); setSelected([]); }} className={inputClass} style={inputStyle}>
            <option value="">choose a connection</option>
            {(connections ?? []).filter((c) => c.enabled).map((c) => (<option key={c.id} value={c.id}>{c.name}</option>))}
          </select>
        </label>
      </div>

      {connectionId && schema && templatesFailed && (
        <p style={{ color: "var(--bad)" }}>Couldn&apos;t load templates.</p>
      )}

      {connectionId && schema && !templatesFailed && templates && (templates.templates ?? []).length === 0 && (
        <EmptyTemplates context="Printing from a connector needs a template to render each item into." />
      )}

      {connectionId && schema && (templates?.templates ?? []).length > 0 && (
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Template</span>
          <select aria-label="template" value={templateId} onChange={(e) => setTemplateId(e.target.value)} className={inputClass} style={inputStyle}>
            <option value="">choose a template</option>
            {(templates?.templates ?? []).map((t) => (<option key={t.id} value={t.id}>{t.name}</option>))}
          </select>
        </label>
      )}

      {connectionId && schema && detail && conn && (
        <Composer
          key={`${connectionId}:${detail.id}`}
          connectionId={connectionId}
          connectorId={conn.connector}
          schema={schema}
          detail={detail}
          stale={isPlaceholderData}
          selected={selected}
          printers={printers ?? []}
        />
      )}

      {connectionId && schema && (
        <ConnectorBrowser key={connectionId} connectionId={connectionId} schema={schema} selected={selected} onSelectedChange={setSelected} />
      )}
    </div>
  );
}

function Composer({
  connectionId, connectorId, schema, detail, stale, selected, printers,
}: {
  connectionId: string;
  connectorId: string;
  schema: ConnectorSchema;
  detail: TemplateDetail;
  stale?: boolean;
  selected: SelectedRow[];
  printers: { id: string; name: string }[];
}) {
  const { push } = useToast();
  const connectorKeys = useMemo(() => [...new Set(schema.resources.flatMap((r) => r.columns.map((c) => c.key)))], [schema]);
  const templateFields = useMemo(() => detail.inputs.all.map((i) => i.name), [detail]);
  const [mapping, setMapping] = useState<FieldMapping>(() => defaultMapping(templateFields, connectorKeys));

  const [rows, setRows] = useState<LabelGridRow[]>([]);
  const rowsRef = useRef(rows);
  const commitRows = (next: LabelGridRow[]) => { rowsRef.current = next; setRows(next); };

  const [copies, setCopies] = useState(1);
  const [startSlot, setStartSlot] = useState(0);
  const [printer, setPrinter] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [selectedRowId, setSelectedRowId] = useState<string | undefined>(undefined);

  const { getRowInputs, pending: rowsPending } = useBatchRowInputs(
    detail.id,
    rows,
    detail.inputs.default,
  );

  const isSheet = detail.format.type === "sheet";
  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;

  const requiredUnion = useMemo(() => {
    const set = new Set<string>();
    if (rows.length === 0) {
      for (const input of detail.inputs.default) set.add(input.name);
    } else {
      for (const row of rows) {
        const inputs = getRowInputs(row.id) ?? detail.inputs.default;
        for (const input of inputs) set.add(input.name);
      }
    }
    return [...set];
  }, [rows, detail, getRowInputs]);

  const displayedFields = requiredUnion;

  const cellInput = (row: LabelGridRow, field: string): InputSpec | undefined => {
    const inputs = getRowInputs(row.id);
    if (!inputs) return { name: field, control: "text" };
    return inputs.find((i) => i.name === field);
  };

  const validateRow = (row: LabelGridRow): LabelGridRow["validation"] => {
    const field: Record<string, string> = {};
    const inputs = getRowInputs(row.id) ?? detail.inputs.default;
    for (const input of inputs) {
      const valStr = row.data[input.name] !== undefined && row.data[input.name] !== null ? String(row.data[input.name]) : "";
      if (input.control === "datetime" || input.control === "date") {
        const dtErr = datetimeCellError(valStr);
        if (dtErr) {
          field[input.name] = dtErr;
        } else if (valStr.trim().length === 0) {
          if (input.default_error?.message) {
            field[input.name] = input.default_error.message;
          } else if (input.required) {
            field[input.name] = "required";
          }
        }
      } else if (valStr.length === 0) {
        if (input.default_error?.message) {
          field[input.name] = input.default_error.message;
        } else if (input.required) {
          field[input.name] = "required";
        }
      }
    }
    return Object.keys(field).length ? { field } : {};
  };

  const rowInvalid = (row: LabelGridRow): boolean => !!validateRow(row).field;
  const viewRows = rows.map((row) => ({ ...row, validation: validateRow(row) }));
  const hasErrors = viewRows.some(rowInvalid);

  // Keep selectedRowId pointing at a valid row. Fall back to first valid (or undefined) derived each
  // render so no effect is needed: the canonical state is `selectedRowId`.
  const firstValidId = rows.find((r) => !rowInvalid(r))?.id;
  const resolvedSelectedId = rows.some((r) => r.id === selectedRowId) ? selectedRowId : firstValidId;

  // Build the resolved label for the selected row using the same resolution the submit path uses.
  const selRow = rows.find((r) => r.id === resolvedSelectedId);
  const previewData = selRow
    ? pruneDataForSubmit(selRow.data, getRowInputs(selRow.id) ?? detail.inputs.default)
    : undefined;
  const previewLabel = previewData ? { data: previewData } : undefined;

  const preview = useRowPreview({
    templateId: detail.id,
    format: isSheet ? "sheet" : "single",
    label: previewLabel,
    startSlot: isSheet ? startSlot : undefined,
  });
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
      const materialized = await materializeConnection(connectionId, { rows: selected.map(({ resource, key }) => ({ resource, key })), fields, expansion: "as_listed" });
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
    if (stale) return; // detail is the previous template during a switch (keepPreviousData); do not submit
    const snapshot = rowsRef.current;
    if (snapshot.length === 0) return;
    if (rowsPending) { setFormError("Resolving row inputs; please wait."); return; }
    if (snapshot.some(rowInvalid)) { setFormError("Fix the highlighted rows before running."); return; }
    if (expandedCount(snapshot.length, copies) > MAX_BATCH_LABELS) { setFormError(`Too many labels (over the ${MAX_BATCH_LABELS} limit).`); return; }
    if (mode === "print" && !printer) { setFormError("Select a printer to print."); return; }
    setBusy(true);
    commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
    const submittedIds = rowsRef.current.map((r) => r.id);
    const submittedCopies = copies;
    const idForExpandedIndex = (index: number): string | undefined => submittedIds[sourceRowForExpandedIndex(index, submittedCopies)];
    try {
      const labels = rowsRef.current.flatMap((r) => {
        const pruned = pruneDataForSubmit(r.data, getRowInputs(r.id) ?? detail.inputs.default);
        return Array.from({ length: submittedCopies }, () => ({ data: pruned }));
      });
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
          </div>

          <LabelGrid
            rows={viewRows}
            fields={displayedFields}
            cellInput={cellInput}
            onRowsChange={(next, { indexes }) => {
              const dirty = new Set(indexes);
              commitRows(next.map((r, i) => ({ ...r, validation: {}, annotation: dirty.has(i) ? undefined : r.annotation })));
              setFormError(null);
            }}
            onDuplicate={(id) => { commitRows(duplicateRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
            onRemove={(id) => { commitRows(removeRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
            disabled={busy}
            selectedRowId={resolvedSelectedId}
            onSelectRow={setSelectedRowId}
          />

          <PreviewPane name={detail.name} format={isSheet ? "sheet" : "single"} preview={preview} />

          <div className="sticky bottom-0 flex flex-wrap items-center gap-3 border-t py-3" style={{ background: "var(--bg)", borderColor: "var(--border)" }}>
            <button type="button" onClick={() => run("print")} disabled={busy || overCap || hasErrors || !printer || stale} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink)" }}>Print</button>
            <button type="button" onClick={() => run("download")} disabled={busy || overCap || hasErrors || stale} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Download</button>
            <span className="text-sm" style={{ color: "var(--muted)" }}>{total} labels</span>
            {overCap && <span style={{ color: "var(--bad)" }}>over the {MAX_BATCH_LABELS}-label limit</span>}
            {formError && <span style={{ color: "var(--bad)" }}>{formError}</span>}
          </div>
        </>
      )}
    </div>
  );
}
