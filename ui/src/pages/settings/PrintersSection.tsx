import { useMemo, useState } from "react";
import {
  usePrinters,
  useSavePrinter,
  useDeletePrinter,
  useSetDefaultPrinter,
  useClearDefaultPrinter,
  useProbePrinter,
} from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { Printer, ProbeResult } from "../../api/types";

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

// Carry forward the non-secret auth config (username/ca_cert/insecure) on edit so saving through the
// auth-less card does not strip it (a PUT replaces config, and the server only merges the write-only
// password). Surfacing these fields for editing is #118.
function cupsAuthConfig(p: Printer): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  const config = p.config;
  if (typeof config === "object" && config !== null) {
    const c = config as Record<string, unknown>;
    if (typeof c.username === "string") out.username = c.username;
    if (typeof c.ca_cert === "string") out.ca_cert = c.ca_cert;
    if (c.insecure === true) out.insecure = true;
  }
  return out;
}

function PrinterForm({ initial, onClose }: { initial: Printer | null; onClose: () => void }) {
  const isNew = initial === null;
  const [id, setId] = useState(initial?.id ?? "");
  const [name, setName] = useState(initial?.name ?? "");
  const [uri, setUri] = useState(initial ? cupsUri(initial) : "");
  // "auto" means: omit from config so the printer's reported value is negotiated at print time.
  const [colorMode, setColorMode] = useState(initial ? cupsRenderStringField(initial, "color_mode") || "auto" : "auto");
  const [resolution, setResolution] = useState(initial ? cupsRenderStringField(initial, "resolution") : "");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [probeRes, setProbeRes] = useState<ProbeResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const save = useSavePrinter();
  const probe = useProbePrinter();
  const { push } = useToast();

  // Auth config is not editable here (#118) but is preserved verbatim across an edit.
  const carriedAuth = useMemo(() => (initial ? cupsAuthConfig(initial) : {}), [initial]);

  const buildConfig = (): Record<string, unknown> => {
    const config: Record<string, unknown> = { uri: uri.trim(), ...carriedAuth };
    const render: Record<string, unknown> = {};
    if (colorMode !== "auto") render.color_mode = colorMode;
    if (resolution.trim() !== "") render.resolution = Number(resolution.trim());
    if (Object.keys(render).length > 0) config.render = render;
    return config;
  };

  const onTest = () => {
    if (!/^ipps?:\/\//.test(uri.trim())) {
      setProbeRes({ status: "unreachable", detail: "Enter an ipp:// or ipps:// address first." });
      return;
    }
    probe.mutate(
      { uri: uri.trim(), ...carriedAuth },
      {
        onSuccess: (r) => setProbeRes(r),
        onError: (err) =>
          setProbeRes({ status: "unreachable", detail: err instanceof Error ? err.message : "Probe failed" }),
      },
    );
  };

  const submit = () => {
    if (!ID_RE.test(id)) {
      setError("id must contain only letters, digits, '-' or '_'");
      return;
    }
    if (name.trim() === "") {
      setError("name must not be empty");
      return;
    }
    if (!/^ipps?:\/\//.test(uri.trim())) {
      // Mirror the server's cups uri check (driver.rs) so a bad scheme is caught before the request.
      setError("address must start with ipp:// or ipps://");
      return;
    }
    setError(null);
    const printer: Printer = { id, name: name.trim(), kind: "cups", config: buildConfig(), enabled };
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

  const caps = probeRes?.status === "ok" ? probeRes.capabilities : null;

  return (
    <div className="flex flex-col gap-3 rounded-md border p-4" style={{ borderColor: "var(--border)" }}>
      <div className="flex flex-wrap items-end gap-3">
        {isNew && (
          <label className="flex flex-col gap-1">
            <span className="text-xs" style={{ color: "var(--muted)" }}>id</span>
            <input aria-label="printer id" value={id} onChange={(e) => setId(e.target.value)} className={inputClass} style={inputStyle} />
          </label>
        )}
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>name</span>
          <input aria-label="printer name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>address</span>
          <input
            aria-label="address"
            value={uri}
            onChange={(e) => setUri(e.target.value)}
            placeholder="ipp://printer.local:631/ipp/print"
            className={inputClass}
            style={inputStyle}
          />
        </label>
        <button
          type="button"
          onClick={onTest}
          disabled={probe.isPending}
          className={`${buttonBase} border`}
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          {probe.isPending ? "Testing…" : "Test connection"}
        </button>
        <label className="flex items-center gap-2 pb-2">
          <input type="checkbox" aria-label="enabled" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span className="text-sm">enabled</span>
        </label>
      </div>

      {caps && (
        <div className="rounded-md border px-3 py-2 text-sm" style={{ borderColor: "var(--good, #15803d)", color: "var(--ink)" }}>
          <div className="font-medium">✓ {caps.model ?? "Printer reachable"}</div>
          <div className="text-xs" style={{ color: "var(--muted)" }}>
            {[
              caps.media_width_mm != null ? `${caps.media_width_mm}mm` : null,
              caps.resolution_dpi != null ? `${caps.resolution_dpi} dpi` : null,
              caps.color,
              caps.accepts_png ? "PNG" : null,
            ]
              .filter(Boolean)
              .join(" · ")}
          </div>
          <div className="text-xs" style={{ color: "var(--muted)" }}>Used automatically when printing.</div>
        </div>
      )}
      {probeRes?.status === "unreachable" && (
        <p className="text-sm" style={{ color: "var(--warn, #b45309)" }}>
          Couldn't reach printer: {probeRes.detail}
        </p>
      )}

      <div>
        <button
          type="button"
          onClick={() => setShowAdvanced((v) => !v)}
          className="text-sm underline"
          style={{ color: "var(--muted)" }}
          aria-expanded={showAdvanced}
        >
          {showAdvanced ? "▾" : "▸"} Advanced: override printer settings
        </button>
        {showAdvanced && (
          <div className="mt-2 flex flex-wrap gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-xs" style={{ color: "var(--muted)" }}>color mode</span>
              <select aria-label="color mode" value={colorMode} onChange={(e) => setColorMode(e.target.value)} className={inputClass} style={inputStyle}>
                <option value="auto">auto (use printer)</option>
                <option value="color">color</option>
                <option value="bilevel">bilevel</option>
              </select>
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs" style={{ color: "var(--muted)" }}>resolution (dpi)</span>
              <input type="number" aria-label="print resolution" value={resolution} onChange={(e) => setResolution(e.target.value)} placeholder="auto" className={inputClass} style={inputStyle} />
            </label>
          </div>
        )}
      </div>

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

function PrinterRow({
  printer,
  onEdit,
  onDeleted,
  onSetDefault,
}: {
  printer: Printer;
  onEdit: () => void;
  onDeleted: (id: string) => void;
  onSetDefault: (id: string) => void;
}) {
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
      <td className={td}>
        <input
          type="radio"
          name="default-printer"
          aria-label={`default ${printer.name}`}
          checked={printer.is_default ?? false}
          onChange={() => onSetDefault(printer.id)}
        />
      </td>
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
  const setDefault = useSetDefaultPrinter();
  const clearDefault = useClearDefaultPrinter();
  const currentDefaultId = (printers ?? []).find((p) => p.is_default)?.id;
  const th = "px-3 py-2 text-left text-xs font-medium";
  const td = "px-3 py-2 text-sm";
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
              <th className={th} style={{ color: "var(--muted)" }}>Default</th>
              <th className={th} style={{ color: "var(--muted)" }}></th>
            </tr>
          </thead>
          <tbody>
            {(printers ?? []).map((p) => (
              <PrinterRow
                key={p.id}
                printer={p}
                onEdit={() => setEditing(p)}
                onDeleted={onDeleted}
                onSetDefault={(id) => setDefault.mutate(id)}
              />
            ))}
            <tr style={{ borderTop: "1px solid var(--border)" }}>
              <td className={td} colSpan={4} style={{ color: "var(--muted)" }}>No default printer</td>
              <td className={td}>
                <input
                  type="radio"
                  name="default-printer"
                  aria-label="no default printer"
                  checked={!currentDefaultId}
                  onChange={() => currentDefaultId && clearDefault.mutate(currentDefaultId)}
                />
              </td>
              <td className={td}></td>
            </tr>
          </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
