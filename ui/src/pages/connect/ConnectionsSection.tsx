import { useState } from "react";
import {
  useConnections,
  useSaveConnection,
  useDeleteConnection,
  type Connection,
  type ConnectionInput,
  type FieldTransform,
} from "../../api/connectors";
import { useSettings, useUpdateSetting, useResetSetting } from "../../api/queries";
import { useToast } from "../../app/toast-context";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

const CONNECTOR_RESOURCES: Record<string, string[]> = {
  homebox: ["entities", "locations"],
};

function ConnectionForm({ initial, onClose }: { initial: Connection | null; onClose: () => void }) {
  const isNew = initial === null;
  const [name, setName] = useState(initial?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(initial?.base_url ?? "");
  const [publicUrl, setPublicUrl] = useState(initial?.public_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [transforms, setTransforms] = useState<FieldTransform[]>(initial?.transforms ?? []);
  const [error, setError] = useState<string | null>(null);
  const save = useSaveConnection();
  const { push } = useToast();

  const connectorName = initial?.connector ?? "homebox";
  const availableResources = CONNECTOR_RESOURCES[connectorName] ?? ["entities", "locations"];

  const parseRuleError = (errMsg: string | null): { ruleIndex: number | null; message: string | null } => {
    if (!errMsg) return { ruleIndex: null, message: null };
    const match = errMsg.match(/rule\s+(\d+):\s*(.*)/i);
    if (match) {
      return { ruleIndex: parseInt(match[1], 10), message: match[2] };
    }
    return { ruleIndex: null, message: errMsg };
  };

  const { ruleIndex, message: parsedRuleMessage } = parseRuleError(error);

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
    if (isNew && apiKey.trim() === "") { setError("api key is required"); return; }
    setError(null);
    const input: ConnectionInput = {
      connector: connectorName,
      name: name.trim(),
      base_url: baseUrl.trim(),
      public_url: publicUrl.trim() === "" ? null : publicUrl.trim(),
      enabled,
      transforms,
      ...(apiKey.trim() !== "" ? { credential: apiKey.trim() } : {}),
    };
    save.mutate(
      { input, id: initial?.id },
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
          <select aria-label="connector" value={connectorName} disabled className={inputClass} style={inputStyle}>
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
          <span className="text-xs" style={{ color: "var(--muted)" }}>api key{isNew ? "" : " (leave blank to keep)"}</span>
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
          <button
            type="button"
            onClick={() =>
              setTransforms([
                ...transforms,
                { resource: availableResources[0] ?? "entities", source: "", pattern: "" },
              ])
            }
            className="text-xs underline"
            style={{ color: "var(--ink)" }}
          >
            + Add rule
          </button>
        </div>
        {transforms.map((t, idx) => {
          const isThisRuleError = ruleIndex === idx;
          return (
            <div key={idx} className="flex flex-col gap-1 rounded border p-2" style={{ borderColor: "var(--border)" }}>
              <div className="flex flex-wrap items-center gap-2">
                <label className="flex flex-col gap-1 min-w-[120px]">
                  <span className="text-xs" style={{ color: "var(--muted)" }}>resource</span>
                  <select
                    aria-label={`rule ${idx} resource`}
                    value={t.resource}
                    onChange={(e) => {
                      const updated = [...transforms];
                      updated[idx] = { ...updated[idx], resource: e.target.value };
                      setTransforms(updated);
                    }}
                    className={inputClass}
                    style={inputStyle}
                  >
                    {availableResources.map((r) => (
                      <option key={r} value={r}>
                        {r}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="flex flex-1 flex-col gap-1 min-w-[150px]">
                  <span className="text-xs" style={{ color: "var(--muted)" }}>source</span>
                  <input
                    aria-label={`rule ${idx} source`}
                    value={t.source}
                    onChange={(e) => {
                      const updated = [...transforms];
                      updated[idx] = { ...updated[idx], source: e.target.value };
                      setTransforms(updated);
                    }}
                    placeholder="location"
                    className={inputClass}
                    style={inputStyle}
                  />
                </label>
                <label className="flex flex-1 flex-col gap-1 min-w-[200px]">
                  <span className="text-xs" style={{ color: "var(--muted)" }}>pattern</span>
                  <input
                    aria-label={`rule ${idx} pattern`}
                    value={t.pattern}
                    onChange={(e) => {
                      const updated = [...transforms];
                      updated[idx] = { ...updated[idx], pattern: e.target.value };
                      setTransforms(updated);
                    }}
                    placeholder="^(?<id>[^|]+)\s*\|\s*(?<name>.*)$"
                    className={inputClass}
                    style={inputStyle}
                  />
                </label>
                <button
                  type="button"
                  aria-label={`remove rule ${idx}`}
                  onClick={() => setTransforms(transforms.filter((_, i) => i !== idx))}
                  className="self-end pb-2 text-xs hover:underline"
                  style={{ color: "var(--bad)" }}
                >
                  Remove
                </button>
              </div>
              {isThisRuleError && (
                <p className="text-xs" style={{ color: "var(--bad)" }}>
                  {parsedRuleMessage}
                </p>
              )}
            </div>
          );
        })}
      </div>

      {ruleIndex === null && error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      <div className="flex gap-3">
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink)" }}>Save</button>
        <button type="button" onClick={onClose} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Cancel</button>
      </div>
    </div>
  );
}

function ConnectionRow({ conn, onEdit, onDeleted }: { conn: Connection; onEdit: () => void; onDeleted: (id: string) => void }) {
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
                onSuccess: () => { push({ kind: "ok", message: `Deleted ${conn.name}` }); onDeleted(conn.id); },
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
  const onDeleted = (id: string) => { if (editing !== null && editing !== "new" && editing.id === id) setEditing(null); };

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
              <ConnectionRow key={c.id} conn={c} onEdit={() => setEditing(c)} onDeleted={onDeleted} />
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
