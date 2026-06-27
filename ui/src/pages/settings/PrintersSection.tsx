import { useState } from "react";
import { usePrinters, useSavePrinter, useDeletePrinter } from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { Printer } from "../../api/types";

const ID_RE = /^[A-Za-z0-9_-]+$/; // mirrors the server's accepted printer-id charset
const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function cupsUri(p: Printer): string {
  // config is `unknown`; narrow with a guard (not an assertion) and only accept a string uri.
  const config = p.config;
  if (typeof config === "object" && config !== null && "uri" in config) {
    const uri = (config as { uri?: unknown }).uri;
    if (typeof uri === "string") return uri;
  }
  return "";
}

// Reads a plain string field from a cups config for form pre-fill. Never call this for write-only
// secrets (e.g. "password"): the API redacts those, so they are absent here, and seeding a form field
// from them must not become a path that echoes a secret back to the server.
function cupsStringField(p: Printer, field: string): string {
  const config = p.config;
  if (typeof config === "object" && config !== null && field in config) {
    const val = (config as Record<string, unknown>)[field];
    if (typeof val === "string") return val;
  }
  return "";
}

function cupsInsecure(p: Printer): boolean {
  const config = p.config;
  if (typeof config === "object" && config !== null && "insecure" in config) {
    return (config as { insecure?: unknown }).insecure === true;
  }
  return false;
}

function cupsRenderStringField(p: Printer, field: string): string {
  const config = p.config;
  if (typeof config !== "object" || config === null || !("render" in config)) return "";
  const render = (config as Record<string, unknown>).render;
  if (typeof render !== "object" || render === null || !(field in render)) return "";
  const val = (render as Record<string, unknown>)[field];
  if (typeof val === "string") return val;
  if (typeof val === "number") return String(val);
  return "";
}

function PrinterForm({ initial, onClose }: { initial: Printer | null; onClose: () => void }) {
  const isNew = initial === null;
  const [id, setId] = useState(initial?.id ?? "");
  const [name, setName] = useState(initial?.name ?? "");
  const [uri, setUri] = useState(initial ? cupsUri(initial) : "");
  const [username, setUsername] = useState(initial ? cupsStringField(initial, "username") : "");
  // password always starts blank: the API never returns it (write-only secret).
  const [password, setPassword] = useState("");
  const [caCert, setCaCert] = useState(initial ? cupsStringField(initial, "ca_cert") : "");
  const [insecure, setInsecure] = useState(initial ? cupsInsecure(initial) : false);
  const [colorMode, setColorMode] = useState(initial ? (cupsRenderStringField(initial, "color_mode") || "color") : "color");
  const [resolution, setResolution] = useState(initial ? cupsRenderStringField(initial, "resolution") : "");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [error, setError] = useState<string | null>(null);
  const save = useSavePrinter();
  const { push } = useToast();

  const submit = () => {
    if (!ID_RE.test(id)) {
      setError("id must contain only letters, digits, '-' or '_'");
      return;
    }
    if (name.trim() === "") {
      setError("name must not be empty");
      return;
    }
    if (uri.trim() === "") {
      setError("cups uri must not be empty");
      return;
    }
    if (!/^ipps?:\/\//.test(uri.trim())) {
      // Mirror the server's cups uri check (driver.rs) so a bad scheme is caught before the request.
      setError("cups uri must start with ipp:// or ipps://");
      return;
    }
    setError(null);
    const config: Record<string, unknown> = { uri: uri.trim() };
    if (username.trim() !== "") config.username = username.trim();
    if (password !== "") config.password = password;
    if (caCert.trim() !== "") config.ca_cert = caCert.trim();
    if (insecure) config.insecure = true;
    const render: Record<string, unknown> = {};
    if (colorMode !== "color") render.color_mode = colorMode;
    if (resolution.trim() !== "") render.resolution = Number(resolution.trim());
    if (Object.keys(render).length > 0) config.render = render;
    const printer: Printer = { id, name: name.trim(), kind: "cups", config, enabled };
    save.mutate(
      { printer, isNew },
      {
        onSuccess: () => {
          push({ kind: "ok", message: `Saved ${id}` });
          onClose();
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Save failed";
          setError(message);
          push({ kind: "error", message });
        },
      },
    );
  };

  return (
    <div className="flex flex-col gap-3 rounded-md border p-4" style={{ borderColor: "var(--border)" }}>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>printer id</span>
          <input aria-label="printer id" value={id} disabled={!isNew} onChange={(e) => setId(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>printer name</span>
          <input aria-label="printer name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>kind</span>
          <select aria-label="printer kind" value="cups" disabled className={inputClass} style={inputStyle}>
            <option value="cups">cups</option>
          </select>
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>cups uri</span>
          <input aria-label="cups uri" value={uri} onChange={(e) => setUri(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex items-center gap-2 self-end pb-2">
          <input type="checkbox" aria-label="enabled" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span className="text-sm">enabled</span>
        </label>
      </div>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>username</span>
          <input aria-label="username" value={username} onChange={(e) => setUsername(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>password</span>
          <input type="password" aria-label="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="leave blank to keep current" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>ca cert</span>
          <textarea aria-label="ca cert" value={caCert} onChange={(e) => setCaCert(e.target.value)} rows={3} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex items-center gap-2 self-end pb-2">
          <input type="checkbox" aria-label="insecure" checked={insecure} onChange={(e) => setInsecure(e.target.checked)} />
          <span className="text-sm">skip TLS verification (insecure)</span>
        </label>
      </div>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>color mode</span>
          <select aria-label="color mode" value={colorMode} onChange={(e) => setColorMode(e.target.value)} className={inputClass} style={inputStyle}>
            <option value="color">color</option>
            <option value="bilevel">bilevel</option>
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>print resolution</span>
          <input type="number" aria-label="print resolution" value={resolution} onChange={(e) => setResolution(e.target.value)} placeholder="e.g. 203" className={inputClass} style={inputStyle} />
        </label>
      </div>
      {(username.trim() !== "" || password !== "") ? (
        !/^ipps:\/\//.test(uri.trim()) ? (
          <p className="text-xs" style={{ color: "var(--warn, #b45309)" }}>
            Credentials are sent unencrypted over ipp://; use ipps://.
          </p>
        ) : null
      ) : null}
      {insecure && (username.trim() !== "" || password !== "") ? (
        <p className="text-xs" style={{ color: "var(--warn, #b45309)" }}>
          Skipping TLS verification with credentials exposes them to man-in-the-middle theft.
        </p>
      ) : null}
      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      <div className="flex gap-3">
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}>
          Save
        </button>
        <button type="button" onClick={onClose} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function PrinterRow({ printer, onEdit, onDeleted }: { printer: Printer; onEdit: () => void; onDeleted: (id: string) => void }) {
  const [confirming, setConfirming] = useState(false);
  const remove = useDeletePrinter();
  const { push } = useToast();
  const td = "px-3 py-2 text-sm";
  return (
    <tr style={{ borderTop: "1px solid var(--border)" }}>
      <td className={td}>{printer.name}</td>
      <td className={`${td} font-mono`}>{printer.kind}</td>
      <td className={`${td} font-mono`}>{cupsUri(printer)}</td>
      <td className={td}>{printer.enabled ? "yes" : "no"}</td>
      <td className={`${td} flex gap-2`}>
        <button type="button" onClick={onEdit} className="underline" style={{ color: "var(--ink)" }}>Edit</button>
        {confirming ? (
          <>
            <button
              type="button"
              disabled={remove.isPending}
              onClick={() =>
                remove.mutate(printer.id, {
                  onSuccess: () => {
                    push({ kind: "ok", message: `Deleted ${printer.id}` });
                    onDeleted(printer.id);
                  },
                  onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Delete failed" }),
                })
              }
              style={{ color: "var(--bad)" }}
            >
              Confirm
            </button>
            <button type="button" onClick={() => setConfirming(false)} style={{ color: "var(--muted)" }}>Cancel</button>
          </>
        ) : (
          <button type="button" onClick={() => setConfirming(true)} style={{ color: "var(--bad)" }}>Delete</button>
        )}
      </td>
    </tr>
  );
}

export function PrintersSection() {
  const { data: printers, isPending, isError } = usePrinters();
  const [editing, setEditing] = useState<Printer | "new" | null>(null);
  const th = "px-3 py-2 text-left text-xs font-medium";
  // If the printer currently being edited is deleted, close the now-stale form (a Save would 404).
  const onDeleted = (id: string) => {
    if (editing !== null && editing !== "new" && editing.id === id) setEditing(null);
  };

  return (
    <section className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Printers</h2>
        <button
          type="button"
          onClick={() => setEditing("new")}
          className={`${buttonBase} border`}
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          Add printer
        </button>
      </div>

      {editing !== null && (
        <PrinterForm
          key={editing === "new" ? "new" : editing.id}
          initial={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
        />
      )}

      {isPending ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>Loading printers...</p>
      ) : isError ? (
        <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load printers.</p>
      ) : (printers ?? []).length === 0 ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>No printers configured.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}>Name</th>
              <th className={th} style={{ color: "var(--muted)" }}>Kind</th>
              <th className={th} style={{ color: "var(--muted)" }}>URI</th>
              <th className={th} style={{ color: "var(--muted)" }}>Enabled</th>
              <th className={th} style={{ color: "var(--muted)" }}></th>
            </tr>
          </thead>
          <tbody>
            {(printers ?? []).map((p) => (
              <PrinterRow key={p.id} printer={p} onEdit={() => setEditing(p)} onDeleted={onDeleted} />
            ))}
          </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
