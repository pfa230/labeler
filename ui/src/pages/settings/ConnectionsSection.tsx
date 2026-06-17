import { useState } from "react";
import { useConnections, useSaveConnection, useDeleteConnection, type Connection, type ConnectionInput } from "../../api/connectors";
import { useToast } from "../../app/toast-context";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function ConnectionForm({ initial, onClose }: { initial: Connection | null; onClose: () => void }) {
  const isNew = initial === null;
  const [name, setName] = useState(initial?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(initial?.base_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [error, setError] = useState<string | null>(null);
  const save = useSaveConnection();
  const { push } = useToast();

  const submit = () => {
    if (name.trim() === "") { setError("name must not be empty"); return; }
    let url: URL;
    try { url = new URL(baseUrl.trim()); } catch { setError("base url must be a valid URL"); return; }
    if (url.protocol !== "http:" && url.protocol !== "https:") { setError("base url must be http or https"); return; }
    if (isNew && apiKey.trim() === "") { setError("api key is required"); return; }
    setError(null);
    const input: ConnectionInput = {
      connector: initial?.connector ?? "homebox",
      name: name.trim(),
      base_url: baseUrl.trim(),
      enabled,
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
          <select aria-label="connector" value={initial?.connector ?? "homebox"} disabled className={inputClass} style={inputStyle}>
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
          <span className="text-xs" style={{ color: "var(--muted)" }}>api key{isNew ? "" : " (leave blank to keep)"}</span>
          <input aria-label="api key" type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex items-center gap-2 self-end pb-2">
          <input type="checkbox" aria-label="enabled" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span className="text-sm">enabled</span>
        </label>
      </div>
      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      <div className="flex gap-3">
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}>Save</button>
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
  const [editing, setEditing] = useState<Connection | "new" | null>(null);
  const th = "px-3 py-2 text-left text-xs font-medium";
  const onDeleted = (id: string) => { if (editing !== null && editing !== "new" && editing.id === id) setEditing(null); };
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
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}>Name</th>
              <th className={th} style={{ color: "var(--muted)" }}>Connector</th>
              <th className={th} style={{ color: "var(--muted)" }}>Base URL</th>
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
      )}
    </section>
  );
}
