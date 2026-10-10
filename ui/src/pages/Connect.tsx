import { useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useIsMutating } from "@tanstack/react-query";
import { useConnections, useConnectorSchema, materializeConnection, type ConnectorSchema, type SelectedRow } from "../api/connectors";
import { ConnectorBrowser } from "./connect/ConnectorBrowser";
import { useTemplates, useTemplate, usePrinters, useSettings } from "../api/queries";
import { EmptyTemplates } from "../components/EmptyTemplates";
import { datetimeCellError } from "../lib/templateFields";
import { defaultMapping, mappedConnectorKeys, rowsFromMaterialized, validateMapping, type FieldMapping } from "../lib/connectorRows";
import {
  MAX_BATCH_LABELS, expandedCount, sourceRowForExpandedIndex,
  duplicateRow, removeRow, resolveLabels, sheetPreviewBlock, type LabelGridRow,
} from "../lib/labelGrid";
import { LabelGrid } from "../components/LabelGrid";
import { PreviewPane } from "../components/PreviewPane";
import { useRowPreview } from "../lib/rowPreview";
import { useSheetPreview } from "../lib/sheetPreview";
import { getOwnKey, pruneDataForSubmit } from "../lib/labelInputs";
import { ApiError, printBatch, renderBatch, saveBlob, sentMessage } from "../api/client";
import { useToast } from "../app/toast-context";
import type { TemplateDetail } from "../api/types";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };
const buttonBase = "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const MATERIALIZE_CAP = 200; // backend /materialize rejects more than this in one call (400 row_limit_exceeded)

export function Connect() {
  const { data: connections, isError: connectionsFailed } = useConnections();
  const { data: settings, isError: settingsFailed } = useSettings();
  const { data: templates, isError: templatesFailed } = useTemplates();
  const { data: printers } = usePrinters();

  const isMutating = useIsMutating({ mutationKey: ["connection"] });
  const isWaiting = isMutating > 0;

  const [selectedConnectionId, setSelectedConnectionId] = useState<string | null>(null);
  const [latchedConnectionId, setLatchedConnectionId] = useState<string | null>(null);
  const [templateId, setTemplateId] = useState("");
  const [selected, setSelected] = useState<SelectedRow[]>([]);

  if (latchedConnectionId === null && !isWaiting) {
    if (connectionsFailed) {
      setLatchedConnectionId("");
    } else if (connections !== undefined && (settings !== undefined || settingsFailed)) {
      const defaultId =
        typeof settings?.default_connection_id?.value === "string"
          ? settings.default_connection_id.value
          : null;
      const resolved = connections.find((c) => c.id === defaultId) ?? connections[0];
      setLatchedConnectionId(resolved?.id ?? "");
    }
  }

  const effectiveId = selectedConnectionId !== null ? selectedConnectionId : (latchedConnectionId ?? "");
  const connectionId = !isWaiting && (connections !== undefined || connectionsFailed)
    ? (connections && !connectionsFailed ? (connections.some((c) => c.id === effectiveId) ? effectiveId : "") : effectiveId)
    : "";

  if (connections !== undefined && !connectionsFailed && effectiveId !== "") {
    const isOffered = connections.some((c) => c.id === effectiveId);
    if (!isOffered) {
      setSelectedConnectionId("");
      setSelected([]);
    }
  }

  const { data: schema } = useConnectorSchema(connectionId);
  const { data: detail, isPlaceholderData } = useTemplate(templateId);

  const conn = (connections ?? []).find((c) => c.id === connectionId);

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Connect</h1>

      {isWaiting && (
        <p className="text-sm" style={{ color: "var(--muted)" }}>Waiting...</p>
      )}

      {!isWaiting && connectionsFailed && (
        <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load connections.</p>
      )}

      {!isWaiting && connections !== undefined && !connectionsFailed && connections.length === 0 && (
        <p className="text-sm" style={{ color: "var(--muted)" }}>
          No connections configured.{" "}
          <Link to="/connections/new" state={{ from: "/connect" }} className="underline" style={{ color: "var(--ink)" }}>
            Add connection
          </Link>
        </p>
      )}

      <div className="flex flex-wrap items-center gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Connection</span>
          <select
            aria-label="connection"
            disabled={isWaiting}
            value={connectionId}
            onChange={(e) => { setSelectedConnectionId(e.target.value); setSelected([]); }}
            className={inputClass}
            style={inputStyle}
          >
            <option value="">choose a connection</option>
            {(connections ?? []).map((c) => (<option key={c.id} value={c.id}>{c.name}</option>))}
          </select>
        </label>
        <Link to="/connections" state={{ from: "/connect" }} className="text-sm underline self-end pb-2" style={{ color: "var(--ink)" }}>
          Manage connections
        </Link>
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
        <ConnectorBrowser
          key={connectionId}
          connectionId={connectionId}
          schema={schema}
          selected={selected}
          onSelectedChange={setSelected}
        />
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
  const connectorColumns = useMemo(() => schema.resources.flatMap((r) => r.columns), [schema]);
  const connectorKeys = useMemo(() => [...new Set(connectorColumns.map((c) => c.key))], [connectorColumns]);
  const templateFields = useMemo(() => detail.params.map((p) => p.name), [detail]);
  const [mapping, setMapping] = useState<FieldMapping>(() => defaultMapping(detail.params, connectorColumns));
  const mappingErrors = useMemo(
    () => validateMapping(mapping, detail.params, connectorColumns),
    [mapping, detail, connectorColumns],
  );

  const [rows, setRows] = useState<LabelGridRow[]>([]);
  const rowsRef = useRef(rows);
  const commitRows = (next: LabelGridRow[]) => { rowsRef.current = next; setRows(next); };

  const [copies, setCopies] = useState(1);
  const [startSlot, setStartSlot] = useState(0);
  const [printer, setPrinter] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [selectedRowId, setSelectedRowId] = useState<string | undefined>(undefined);

  const isSheet = detail.format.type === "sheet";
  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;

  const cellInput = (_row: LabelGridRow, field: string) => detail.params.find((p) => p.name === field);

  const validateRow = (row: LabelGridRow): LabelGridRow["validation"] => {
    const field: Record<string, string> = {};
    for (const param of detail.params) {
      if (param.control !== "datetime" && param.control !== "date") continue;
      const held = getOwnKey(row.data, param.name);
      const dtErr = datetimeCellError(held !== undefined && held !== null ? String(held) : "");
      if (dtErr) field[param.name] = dtErr;
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

  const dataFor = (r: LabelGridRow) => pruneDataForSubmit(r.data, detail.params);

  // Build the resolved label for the selected row using the same resolution the submit path uses.
  const selRow = rows.find((r) => r.id === resolvedSelectedId);
  const previewData = selRow ? dataFor(selRow) : undefined;
  const previewLabel = previewData ? { data: previewData } : undefined;

  const total = expandedCount(rows.length, copies);
  const overCap = total > MAX_BATCH_LABELS;

  const invalidPositions = viewRows.flatMap((row, index) => (rowInvalid(row) ? [index + 1] : []));
  const blocked = sheetPreviewBlock(invalidPositions, total);

  const sheetPreview = useSheetPreview(
    { templateId: detail.id, labels: resolveLabels(rows, copies, dataFor), startSlot },
    isSheet && rows.length > 0 && !blocked,
  );
  const rowPreview = useRowPreview({
    templateId: detail.id,
    label: isSheet ? undefined : previewLabel,
  });

  const preview = isSheet ? (blocked ? { loading: false, blocked } : sheetPreview) : rowPreview;

  const addRows = async () => {
    if (selected.length === 0 || mappingErrors.length > 0) return;
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
    if (snapshot.some(rowInvalid)) { setFormError("Fix the highlighted rows before running."); return; }
    if (expandedCount(snapshot.length, copies) > MAX_BATCH_LABELS) { setFormError(`Too many labels (over the ${MAX_BATCH_LABELS} limit).`); return; }
    const printTo = mode === "print" ? printer : undefined;
    if (mode === "print" && !printTo) { setFormError("Select a printer to print."); return; }
    setBusy(true);
    commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
    const submittedIds = rowsRef.current.map((r) => r.id);
    const submittedCopies = copies;
    const idForExpandedIndex = (index: number): string | undefined => submittedIds[sourceRowForExpandedIndex(index, submittedCopies)];
    try {
      const labels = resolveLabels(rowsRef.current, submittedCopies, dataFor);
      const slot = isSheet && startSlot ? { start_slot: startSlot } : {};
      if (!printTo) {
        const { blob, filename } = await renderBatch({ template: detail.id, labels, ...slot });
        saveBlob(blob, filename ?? `${detail.id}.${isSheet ? "pdf" : "zip"}`);
        push({ kind: "ok", message: `Downloaded ${labels.length} labels` });
      } else {
        const summary = await printBatch({ template: detail.id, labels, printer: printTo, ...slot });
        const { failed } = summary;
        const failById = new Map<string, string>();
        for (const f of failed) { const id = idForExpandedIndex(f.index); if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.error}` : f.error); }
        const submitted = new Set(submittedIds);
        commitRows(rowsRef.current.map((row) =>
          submitted.has(row.id)
            ? { ...row, annotation: failById.has(row.id) ? { status: "failed", message: failById.get(row.id) } : { status: "ok" } }
            : row));
        const name = printers.find((p) => p.id === printTo)?.name ?? printTo;
        push({ kind: failed.length ? "error" : "ok", message: sentMessage(summary, name) });
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
        {mappingErrors.map((err) => (
          <p key={err} className="text-sm" style={{ color: "var(--bad)" }}>{err}</p>
        ))}
        <div>
          <button type="button" onClick={addRows} disabled={busy || selected.length === 0 || mappingErrors.length > 0} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
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
            fields={templateFields}
            cellInput={cellInput}
            onRowsChange={(next, { indexes }) => {
              const dirty = new Set(indexes);
              commitRows(next.map((r, i) => ({ ...r, validation: {}, annotation: dirty.has(i) ? undefined : r.annotation })));
              setFormError(null);
            }}
            onDuplicate={(id) => { commitRows(duplicateRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
            onRemove={(id) => { commitRows(removeRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
            disabled={busy}
            selectedRowId={isSheet ? undefined : resolvedSelectedId}
            onSelectRow={isSheet ? undefined : setSelectedRowId}
          />

          <PreviewPane name={detail.name} format={isSheet ? "sheet" : "single"} preview={preview} />

          {/* An action bar over the preview has to be opaque and reach the scrollport floor.
              --bg is defined nowhere, so the background computed to transparent, and at
              bottom-0 the bar stopped 24px short on the Shell's p-6, leaving the preview
              visible through and beneath it. The 1.5rem in the padding is that p-6 again. The
              inset is the Print bar's spelling of the same 2.25rem and is inert until
              index.html asks for viewport-fit=cover, without which env() is 0 (#374).
              z-10 matches the Print page's bar. */}
          <div className="sticky -bottom-6 z-10 flex flex-wrap items-center gap-3 border-t pt-3 pb-[calc(0.75rem_+_1.5rem_+_env(safe-area-inset-bottom))]" style={{ background: "var(--paper)", borderColor: "var(--border)" }}>
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
