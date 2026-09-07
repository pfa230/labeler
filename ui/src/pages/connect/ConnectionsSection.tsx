import { useState, useRef, useEffect } from "react";
import {
  useConnections,
  useSaveConnection,
  useDeleteConnection,
  useConnectorSchema,
  previewTransforms,
  type Connection,
  type ConnectionInput,
  type FieldTransform,
  type TransformPreviewResponse,
} from "../../api/connectors";
import { useSettings, useUpdateSetting, useResetSetting } from "../../api/queries";
import { useToast } from "../../app/toast-context";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function CreateConnectionForm({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [publicUrl, setPublicUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const save = useSaveConnection();
  const { push } = useToast();

  const submit = () => {
    if (name.trim() === "") { setError("name must not be empty"); return; }
    let url: URL;
    try { url = new URL(baseUrl.trim()); } catch { setError("base url must be a valid URL"); return; }
    if (url.protocol !== "http:" && url.protocol !== "https:") { setError("base url must be http or https"); return; }
    if (publicUrl.trim() !== "") {
      let pubUrl: URL;
      try { pubUrl = new URL(publicUrl.trim()); } catch { setError("public url must be a valid URL"); return; }
      if (pubUrl.protocol !== "http:" && pubUrl.protocol !== "https:") { setError("public url must be http or https"); return; }
    }
    if (apiKey.trim() === "") { setError("api key is required"); return; }
    setError(null);
    const input: ConnectionInput = {
      connector: "homebox",
      name: name.trim(),
      base_url: baseUrl.trim(),
      public_url: publicUrl.trim() === "" ? null : publicUrl.trim(),
      enabled,
      credential: apiKey.trim(),
    };
    save.mutate(
      { input },
      {
        onSuccess: () => { push({ kind: "ok", message: `Saved ${input.name}` }); onClose(); },
        onError: (err) => { const message = err instanceof Error ? err.message : "Save failed"; setError(message); push({ kind: "error", message }); },
      },
    );
  };

  return (
    <div className="flex flex-col gap-3 rounded-md border p-4" style={{ borderColor: "var(--border)" }}>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>connector</span>
          <select aria-label="connector" value="homebox" disabled className={inputClass} style={inputStyle}>
            <option value="homebox">homebox</option>
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>name</span>
          <input aria-label="name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>base url</span>
          <input aria-label="base url" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="http://homebox.lan:7745" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>public url</span>
          <input aria-label="public url" value={publicUrl} onChange={(e) => setPublicUrl(e.target.value)} placeholder="https://homebox.example.com" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>api key</span>
          <input aria-label="api key" type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex items-center gap-2 self-end pb-2">
          <input type="checkbox" aria-label="enabled" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span className="text-sm">enabled</span>
        </label>
      </div>

      <div className="flex flex-col gap-2 pt-2 border-t" style={{ borderColor: "var(--border)" }}>
        <span className="text-xs font-medium" style={{ color: "var(--muted)" }}>Field transforms</span>
        <p className="text-xs" style={{ color: "var(--muted)" }}>
          Transform rules can be added after saving the connection.
        </p>
      </div>

      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      <div className="flex gap-3">
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink)" }}>Save</button>
        <button type="button" onClick={onClose} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Cancel</button>
      </div>
    </div>
  );
}

function EditConnectionForm({ initial, onClose }: { initial: Connection; onClose: () => void }) {
  const [name, setName] = useState(initial.name);
  const [baseUrl, setBaseUrl] = useState(initial.base_url);
  const [publicUrl, setPublicUrl] = useState(initial.public_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [enabled, setEnabled] = useState(initial.enabled);
  const [transforms, setTransforms] = useState<FieldTransform[]>(initial.transforms ?? []);
  const [listRevision, setListRevision] = useState(0);
  const listRevisionRef = useRef(0);
  const ruleSeqRef = useRef<Record<number, number>>({});
  const [previewResults, setPreviewResults] = useState<{
    ruleIndex: number;
    revision: number;
    seq: number;
    response: TransformPreviewResponse;
  } | null>(null);
  const [previewLoadingRule, setPreviewLoadingRule] = useState<number | null>(null);
  const [ruleErrors, setRuleErrors] = useState<Record<number, string>>({});
  const [formError, setFormError] = useState<string | null>(null);

  const schemaQuery = useConnectorSchema(initial.id);
  const save = useSaveConnection();
  const { push } = useToast();

  const isDirtyDetails = baseUrl.trim() !== initial.base_url || apiKey.trim() !== "";
  const isSchemaError = schemaQuery.isError;
  const isSuspended = isDirtyDetails || isSchemaError;
  const suspendedReason = isSchemaError
    ? "Failed to load connector schema. Transform rules cannot be edited."
    : isDirtyDetails
    ? "Connection details must be saved first."
    : null;

  const updateRule = (idx: number, patch: Partial<FieldTransform>) => {
    const updated = [...transforms];
    updated[idx] = { ...updated[idx], ...patch };
    setTransforms(updated);
    setListRevision((r) => r + 1);
    listRevisionRef.current += 1;
    setPreviewResults(null);
    setPreviewLoadingRule(null);
    setRuleErrors({});
  };

  const addRule = () => {
    const firstRes = schemaQuery.data?.resources[0];
    if (!firstRes) return;
    const firstEligible = firstRes.columns.filter((c) => c.transform_source)[0];
    const defaultSource = firstEligible
      ? firstEligible.key
      : (firstRes.dynamic_source_prefix ?? "");
    const newRule: FieldTransform = {
      resource: firstRes.id,
      source: defaultSource,
      pattern: "",
    };
    setTransforms([...transforms, newRule]);
    setListRevision((r) => r + 1);
    listRevisionRef.current += 1;
    setPreviewResults(null);
    setPreviewLoadingRule(null);
    setRuleErrors({});
  };

  const removeRule = (idx: number) => {
    setTransforms(transforms.filter((_, i) => i !== idx));
    setListRevision((r) => r + 1);
    listRevisionRef.current += 1;
    setPreviewResults(null);
    setPreviewLoadingRule(null);
    setRuleErrors({});
  };

  const handlePreview = async (idx: number) => {
    const reqRevision = listRevisionRef.current;
    const seq = (ruleSeqRef.current[idx] ?? 0) + 1;
    ruleSeqRef.current[idx] = seq;
    setPreviewLoadingRule(idx);
    setRuleErrors({});
    setFormError(null);

    try {
      const res = await previewTransforms(initial.id, {
        transforms,
        rule: idx,
      });
      if (listRevisionRef.current === reqRevision && ruleSeqRef.current[idx] === seq) {
        setPreviewResults({ ruleIndex: idx, revision: reqRevision, seq, response: res });
      }
    } catch (err: unknown) {
      if (listRevisionRef.current === reqRevision && ruleSeqRef.current[idx] === seq) {
        const errMsg = err instanceof Error ? err.message : "Preview failed";
        const match = errMsg.match(/rule\s+(\d+):\s*(.*)/i);
        if (match) {
          const offendingRule = parseInt(match[1], 10);
          setRuleErrors({ [offendingRule]: match[2] });
        } else {
          setFormError(errMsg);
        }
      }
    } finally {
      if (ruleSeqRef.current[idx] === seq) {
        setPreviewLoadingRule((current) => (current === idx ? null : current));
      }
    }
  };

  const submit = () => {
    if (name.trim() === "") { setFormError("name must not be empty"); return; }
    let url: URL;
    try { url = new URL(baseUrl.trim()); } catch { setFormError("base url must be a valid URL"); return; }
    if (url.protocol !== "http:" && url.protocol !== "https:") { setFormError("base url must be http or https"); return; }
    if (publicUrl.trim() !== "") {
      let pubUrl: URL;
      try { pubUrl = new URL(publicUrl.trim()); } catch { setFormError("public url must be a valid URL"); return; }
      if (pubUrl.protocol !== "http:" && pubUrl.protocol !== "https:") { setFormError("public url must be http or https"); return; }
    }
    setFormError(null);
    setRuleErrors({});

    const input: ConnectionInput = {
      connector: initial.connector,
      name: name.trim(),
      base_url: baseUrl.trim(),
      public_url: publicUrl.trim() === "" ? null : publicUrl.trim(),
      enabled,
      ...(apiKey.trim() !== "" ? { credential: apiKey.trim() } : {}),
      ...(!isSuspended && schemaQuery.data ? { transforms } : {}),
    };

    save.mutate(
      { input, id: initial.id },
      {
        onSuccess: () => {
          push({ kind: "ok", message: `Saved ${input.name}` });
          onClose();
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Save failed";
          const match = message.match(/rule\s+(\d+):\s*(.*)/i);
          if (match) {
            const offendingRule = parseInt(match[1], 10);
            setRuleErrors({ [offendingRule]: match[2] });
          } else {
            setFormError(message);
          }
          push({ kind: "error", message });
        },
      },
    );
  };

  const schema = schemaQuery.data;

  return (
    <div className="flex flex-col gap-3 rounded-md border p-4" style={{ borderColor: "var(--border)" }}>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>connector</span>
          <select aria-label="connector" value={initial.connector} disabled className={inputClass} style={inputStyle}>
            <option value="homebox">homebox</option>
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>name</span>
          <input aria-label="name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>base url</span>
          <input aria-label="base url" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="http://homebox.lan:7745" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>public url</span>
          <input aria-label="public url" value={publicUrl} onChange={(e) => setPublicUrl(e.target.value)} placeholder="https://homebox.example.com" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>api key (leave blank to keep)</span>
          <input aria-label="api key" type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex items-center gap-2 self-end pb-2">
          <input type="checkbox" aria-label="enabled" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span className="text-sm">enabled</span>
        </label>
      </div>

      <div className="flex flex-col gap-2 pt-2 border-t" style={{ borderColor: "var(--border)" }}>
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium" style={{ color: "var(--muted)" }}>Field transforms</span>
          {!isSuspended && schema && (
            <button
              type="button"
              onClick={addRule}
              className="text-xs underline"
              style={{ color: "var(--ink)" }}
            >
              + Add rule
            </button>
          )}
        </div>

        {isSuspended ? (
          <div className="flex flex-col gap-2">
            <div className="rounded border p-2 text-xs" style={{ borderColor: "var(--border)", color: "var(--muted)" }}>
              {suspendedReason}
            </div>
            {(initial.transforms ?? []).map((t, idx) => (
              <div key={idx} className="flex flex-wrap items-center gap-2 rounded border p-2 text-xs opacity-75" style={{ borderColor: "var(--border)" }}>
                <span className="font-mono">resource: {t.resource}</span>
                <span className="font-mono">source: {t.source}</span>
                <span className="font-mono">pattern: {t.pattern}</span>
              </div>
            ))}
          </div>
        ) : schemaQuery.isPending ? (
          <p className="text-xs" style={{ color: "var(--muted)" }}>Loading schema...</p>
        ) : schema ? (
          transforms.map((t, idx) => {
            const resourceInSchema = schema.resources.some((r) => r.id === t.resource);
            const currentResource = schema.resources.find((r) => r.id === t.resource);
            const eligibleColumns = currentResource ? currentResource.columns.filter((c) => c.transform_source) : [];
            const prefix = currentResource?.dynamic_source_prefix ?? null;
            const isIncomplete = currentResource?.fields_incomplete ?? false;

            const isEligible = eligibleColumns.some((c) => c.key === t.source);
            const isPrefixed = prefix !== null && (t.source.startsWith(prefix) || (t.source === "" && eligibleColumns.length === 0));
            const isCustomByName = !isEligible && isPrefixed;
            const customName = isCustomByName && prefix ? t.source.slice(prefix.length) : "";
            const isUnavailable = !isEligible && !isCustomByName && t.source !== "";

            return (
              <div key={idx} className="flex flex-col gap-1 rounded border p-2" style={{ borderColor: "var(--border)" }}>
                <div className="flex flex-wrap items-center gap-2">
                  <label className="flex flex-col gap-1 min-w-[120px]">
                    <span className="text-xs" style={{ color: "var(--muted)" }}>resource</span>
                    <select
                      aria-label={`rule ${idx} resource`}
                      value={t.resource}
                      onChange={(e) => {
                        const newResId = e.target.value;
                        const newRes = schema.resources.find((r) => r.id === newResId);
                        const newEligible = newRes ? newRes.columns.filter((c) => c.transform_source) : [];
                        let newSource = "";
                        if (newEligible.length > 0) {
                          newSource = newEligible[0].key;
                        } else if (newRes?.dynamic_source_prefix) {
                          newSource = newRes.dynamic_source_prefix;
                        }
                        updateRule(idx, { resource: newResId, source: newSource });
                      }}
                      className={inputClass}
                      style={inputStyle}
                    >
                      {!resourceInSchema && (
                        <option value={t.resource}>{t.resource} (unavailable)</option>
                      )}
                      {schema.resources.map((r) => (
                        <option key={r.id} value={r.id}>
                          {r.id}
                        </option>
                      ))}
                    </select>
                  </label>

                  <label className="flex flex-1 flex-col gap-1 min-w-[150px]">
                    <span className="text-xs" style={{ color: "var(--muted)" }}>source</span>
                    <select
                      aria-label={`rule ${idx} source`}
                      value={isCustomByName ? "__custom_by_name__" : t.source}
                      onChange={(e) => {
                        if (e.target.value === "__custom_by_name__") {
                          updateRule(idx, { source: prefix ?? "" });
                        } else {
                          updateRule(idx, { source: e.target.value });
                        }
                      }}
                      className={inputClass}
                      style={inputStyle}
                    >
                      {isUnavailable && (
                        <option value={t.source}>{t.source} (unavailable)</option>
                      )}
                      {eligibleColumns.map((c) => (
                        <option key={c.key} value={c.key}>
                          {c.key}
                        </option>
                      ))}
                      {prefix !== null && (
                        <option value="__custom_by_name__">{prefix}&lt;name&gt;</option>
                      )}
                    </select>
                  </label>

                  {isCustomByName && prefix !== null && (
                    <label className="flex flex-1 flex-col gap-1 min-w-[150px]">
                      <span className="text-xs" style={{ color: "var(--muted)" }}>field name</span>
                      <div className="flex items-center">
                        <span
                          className="text-xs px-2 py-2 rounded-l border border-r-0 font-mono select-none"
                          style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--muted)" }}
                        >
                          {prefix}
                        </span>
                        <input
                          aria-label={`rule ${idx} source name`}
                          value={customName}
                          onChange={(e) => {
                            updateRule(idx, { source: `${prefix}${e.target.value}` });
                          }}
                          placeholder="name"
                          className={`${inputClass} rounded-l-none`}
                          style={inputStyle}
                        />
                      </div>
                    </label>
                  )}

                  <label className="flex flex-1 flex-col gap-1 min-w-[200px]">
                    <span className="text-xs" style={{ color: "var(--muted)" }}>pattern</span>
                    <input
                      aria-label={`rule ${idx} pattern`}
                      value={t.pattern}
                      onChange={(e) => updateRule(idx, { pattern: e.target.value })}
                      placeholder="^(?<id>[^|]+)\s*\|\s*(?<name>.*)$"
                      className={inputClass}
                      style={inputStyle}
                    />
                  </label>

                  <div className="flex items-center gap-2 self-end pb-2">
                    <button
                      type="button"
                      aria-label={`preview rule ${idx}`}
                      onClick={() => handlePreview(idx)}
                      className="text-xs font-medium underline"
                      style={{ color: "var(--ink)" }}
                    >
                      {previewLoadingRule === idx ? "Previewing..." : "Preview"}
                    </button>
                    <button
                      type="button"
                      aria-label={`remove rule ${idx}`}
                      onClick={() => removeRule(idx)}
                      className="text-xs hover:underline"
                      style={{ color: "var(--bad)" }}
                    >
                      Remove
                    </button>
                  </div>
                </div>

                {isIncomplete && (
                  <p className="text-xs" style={{ color: "var(--muted)" }}>
                    Field list may be short; fields can still be specified by name.
                  </p>
                )}
                {eligibleColumns.length === 0 && prefix === null && (
                  <p className="text-xs" style={{ color: "var(--muted)" }}>
                    No transformable sources available for this resource.
                  </p>
                )}

                {ruleErrors[idx] && (
                  <p className="text-xs" style={{ color: "var(--bad)" }}>
                    {ruleErrors[idx]}
                  </p>
                )}

                {previewResults && !isSuspended && previewResults.ruleIndex === idx && previewResults.revision === listRevision && (
                  <div
                    className="flex flex-col gap-2 rounded border p-3 text-xs mt-2"
                    style={{ background: "var(--surface)", borderColor: "var(--border)" }}
                  >
                    <div className="font-medium">
                      Matched {previewResults.response.matched_count} of {previewResults.response.row_count} rows
                    </div>
                    <div className="flex flex-col gap-1.5 max-h-60 overflow-y-auto">
                      {previewResults.response.rows.map((row, rIdx) => (
                        <div key={rIdx} className="flex flex-col gap-0.5 border-t pt-1" style={{ borderColor: "var(--border)" }}>
                          <div className="flex items-center gap-2">
                            <span className="font-mono text-xs" style={{ color: "var(--muted)" }}>
                              {row.id.resource}:{row.id.key}
                            </span>
                            {row.source_value === undefined ? (
                              <span className="italic" style={{ color: "var(--muted)" }}>(no source value)</span>
                            ) : (
                              <span className="font-mono">
                                source: &quot;{row.source_value}&quot;
                              </span>
                            )}
                            {row.value_truncated && (
                              <span className="text-[10px] px-1 rounded" style={{ background: "var(--border)", color: "var(--muted)" }}>
                                (shortened)
                              </span>
                            )}
                          </div>
                          <div>
                            {row.matched ? (
                              <div className="font-mono text-[11px]" style={{ color: "var(--ink)" }}>
                                {row.derived && Object.keys(row.derived).length > 0 ? (
                                  Object.entries(row.derived).map(([k, v]) => `${k}="${v}"`).join(", ")
                                ) : (
                                  <span className="italic" style={{ color: "var(--muted)" }}>(no captures)</span>
                                )}
                              </div>
                            ) : (
                              <span className="italic" style={{ color: "var(--muted)" }}>Did not match</span>
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            );
          })
        ) : null}
      </div>

      {formError && <p className="text-sm" style={{ color: "var(--bad)" }}>{formError}</p>}
      <div className="flex gap-3">
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink)" }}>Save</button>
        <button type="button" onClick={onClose} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Cancel</button>
      </div>
    </div>
  );
}

function ConnectionForm({ initial, onClose }: { initial: Connection | null; onClose: () => void }) {
  if (initial === null) {
    return <CreateConnectionForm onClose={onClose} />;
  }
  return <EditConnectionForm initial={initial} onClose={onClose} />;
}

function ConnectionRow({ conn, onEdit }: { conn: Connection; onEdit: () => void }) {
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteConnection();
  const { push } = useToast();
  const td = "px-3 py-2 text-sm";
  return (
    <tr style={{ borderTop: "1px solid var(--border)" }}>
      <td className={td}>{conn.name}</td>
      <td className={`${td} font-mono`}>{conn.connector}</td>
      <td className={`${td} font-mono`}>{conn.base_url}</td>
      <td className={`${td} font-mono`}>{conn.public_url || "-"}</td>
      <td className={td}>{conn.has_credential ? "set" : "none"}</td>
      <td className={td}>{conn.enabled ? "yes" : "no"}</td>
      <td className={`${td} flex gap-2`}>
        <button type="button" onClick={onEdit} className="underline" style={{ color: "var(--ink)" }}>Edit</button>
        {confirming ? (
          <>
            <button type="button" disabled={remove.isPending} onClick={() =>
              remove.mutate(conn.id, {
                onSuccess: () => { push({ kind: "ok", message: `Deleted ${conn.name}` }); },
                onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Delete failed" }),
              })
            } style={{ color: "var(--bad)" }}>Confirm</button>
            <button type="button" onClick={() => setConfirming(false)} style={{ color: "var(--muted)" }}>Cancel</button>
          </>
        ) : (
          <button type="button" onClick={() => setConfirming(true)} style={{ color: "var(--bad)" }}>Delete</button>
        )}
      </td>
    </tr>
  );
}

export function ConnectionsSection() {
  const { data: connections, isPending, isError } = useConnections();
  const { data: settings } = useSettings();
  const updateSetting = useUpdateSetting();
  const resetSetting = useResetSetting();
  const { push } = useToast();
  const [editing, setEditing] = useState<Connection | "new" | null>(null);
  const th = "px-3 py-2 text-left text-xs font-medium";

  useEffect(() => {
    const onDeleted = (e: Event) => {
      const id = (e as CustomEvent<{ id: string }>).detail?.id;
      if (id) {
        setEditing((cur) => (cur && cur !== "new" && cur.id === id ? null : cur));
      }
    };
    window.addEventListener("labeler:connection-deleted", onDeleted);
    return () => window.removeEventListener("labeler:connection-deleted", onDeleted);
  }, []);

  const storedDefault = settings?.default_connection_id;
  const storedDefaultId = typeof storedDefault?.value === "string" ? storedDefault.value : null;
  const isDefault = storedDefault?.is_default ?? true;
  const matchingConn = storedDefaultId ? (connections ?? []).find((c) => c.id === storedDefaultId) : null;
  // "Unavailable" means the stored id names no connection, which is only knowable once the
  // connections list has actually loaded. While it is pending or failed, `matchingConn` is absent
  // because we do not know yet, not because the connection is gone: reporting a valid default as
  // unavailable invites the operator to "fix" it by clearing a setting that was never broken.
  const connectionsKnown = !isPending && !isError;
  const isDangling = connectionsKnown && storedDefaultId !== null && !matchingConn && !isDefault;

  const handleDefaultChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const val = e.target.value;
    if (!val) {
      resetSetting.mutate("default_connection_id", {
        onSuccess: () => push({ kind: "ok", message: "Default connection reset to default" }),
        onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Failed to clear default connection" }),
      });
    } else {
      updateSetting.mutate(
        { key: "default_connection_id", value: val },
        {
          onSuccess: () => push({ kind: "ok", message: "Default connection saved" }),
          onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Failed to save default connection" }),
        },
      );
    }
  };

  return (
    <section className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Connections</h2>
        <button type="button" onClick={() => setEditing("new")} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Add connection</button>
      </div>
      {editing !== null && (
        <ConnectionForm key={editing === "new" ? "new" : editing.id} initial={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      )}
      {isPending ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>Loading connections...</p>
      ) : isError ? (
        <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load connections.</p>
      ) : (connections ?? []).length === 0 ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>No connections configured.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}>Name</th>
              <th className={th} style={{ color: "var(--muted)" }}>Connector</th>
              <th className={th} style={{ color: "var(--muted)" }}>Base URL</th>
              <th className={th} style={{ color: "var(--muted)" }}>Public URL</th>
              <th className={th} style={{ color: "var(--muted)" }}>API key</th>
              <th className={th} style={{ color: "var(--muted)" }}>Enabled</th>
              <th className={th} style={{ color: "var(--muted)" }}></th>
            </tr>
          </thead>
          <tbody>
            {(connections ?? []).map((c) => (
              <ConnectionRow key={c.id} conn={c} onEdit={() => setEditing(c)} />
            ))}
          </tbody>
          </table>
        </div>
      )}

      <div className="flex flex-col gap-1 max-w-md pt-2 border-t" style={{ borderColor: "var(--border)" }}>
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Default connection</span>
          <select
            aria-label="default connection"
            value={isDefault || !storedDefaultId ? "" : storedDefaultId}
            disabled={updateSetting.isPending || resetSetting.isPending || !connectionsKnown}
            onChange={handleDefaultChange}
            className={inputClass}
            style={inputStyle}
          >
            <option value="">(no default)</option>
            {isDangling && (
              <option value={storedDefaultId}>
                {storedDefaultId} (unavailable)
              </option>
            )}
            {!connectionsKnown && storedDefaultId !== null && !isDefault && (
              <option value={storedDefaultId}>{storedDefaultId}</option>
            )}
            {(connections ?? []).map((c) => (
              <option key={c.id} value={c.id}>
                {c.name} ({c.id}){c.enabled ? "" : " (disabled)"}
              </option>
            ))}
          </select>
        </label>
        <p className="text-xs" style={{ color: "var(--muted)" }}>
          The default connection applies to everyone on this instance.
        </p>
      </div>
    </section>
  );
}
