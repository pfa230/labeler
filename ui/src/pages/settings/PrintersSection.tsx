import { useState } from "react";
import {
  usePrinters,
  useSavePrinter,
  useDeletePrinter,
  useProbePrinter,
  useSettings,
  useUpdateSetting,
  useResetSetting,
} from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { Printer, ProbeResult, RenderProfile } from "../../api/types";

const ID_RE = /^[A-Za-z0-9_-]+$/; // mirrors the server's accepted printer-id charset
const DEFAULT_PRINTER_KEY = "default_printer_id";
const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

type ColorModeChoice = "auto" | NonNullable<RenderProfile["color_mode"]>;

function PrinterForm({ initial, onClose }: { initial: Printer | null; onClose: () => void }) {
  const isNew = initial === null;
  const [id, setId] = useState(initial?.id ?? "");
  const [name, setName] = useState(initial?.name ?? "");
  const [uri, setUri] = useState(initial?.uri ?? "");
  // "auto" means: omit from render so the printer's reported value is negotiated at print time.
  const [colorMode, setColorMode] = useState<ColorModeChoice>(initial?.render?.color_mode ?? "auto");
  const [resolution, setResolution] = useState(initial?.render?.resolution?.toString() ?? "");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [probeRes, setProbeRes] = useState<ProbeResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const save = useSavePrinter();
  const probe = useProbePrinter();
  const { push } = useToast();

  // The form has no fields for these, and a PUT replaces the record, so an edit sends the stored
  // values back. Undefined keys drop out of the JSON body, which is how an unset field is omitted.
  const carried = { username: initial?.username, ca_cert: initial?.ca_cert, insecure: initial?.insecure };

  const buildRender = (): RenderProfile | undefined => {
    const render: RenderProfile = {};
    if (colorMode !== "auto") render.color_mode = colorMode;
    if (resolution.trim() !== "") render.resolution = Number(resolution.trim());
    return Object.keys(render).length > 0 ? render : undefined;
  };

  const onTest = () => {
    if (!/^ipps?:\/\//.test(uri.trim())) {
      setProbeRes({ status: "unreachable", detail: "Enter an ipp:// or ipps:// address first." });
      return;
    }
    probe.mutate(
      { uri: uri.trim(), ...carried },
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
    save.mutate(
      { id, printer: { name: name.trim(), uri: uri.trim(), ...carried, render: buildRender() }, isNew },
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
          <label className="flex w-36 flex-col gap-1">
            <span className="text-xs" style={{ color: "var(--muted)" }}>id</span>
            <input aria-label="printer id" value={id} onChange={(e) => setId(e.target.value)} className={inputClass} style={inputStyle} />
          </label>
        )}
        <label className="flex grow basis-56 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>name</span>
          <input aria-label="printer name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
      </div>

      <div className="flex flex-wrap items-end gap-3">
        <label className="flex grow basis-72 flex-col gap-1">
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
              <select aria-label="color mode" value={colorMode} onChange={(e) => setColorMode(e.target.value as ColorModeChoice)} className={inputClass} style={inputStyle}>
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
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink)" }}>
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
  isDefault,
  onEdit,
  onDeleted,
  onSetDefault,
}: {
  printer: Printer;
  isDefault: boolean;
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
      <td className={`${td} font-mono`}>{printer.uri}</td>
      <td className={td}>
        <input
          type="radio"
          name="default-printer"
          aria-label={`default ${printer.name}`}
          checked={isDefault}
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
  const { data: settings } = useSettings();
  const setDefault = useUpdateSetting();
  const clearDefault = useResetSetting();
  const storedDefault = settings?.[DEFAULT_PRINTER_KEY]?.value;
  const currentDefaultId = typeof storedDefault === "string" ? storedDefault : null;
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
              <th className={th} style={{ color: "var(--muted)" }}>URI</th>
              <th className={th} style={{ color: "var(--muted)" }}>Default</th>
              <th className={th} style={{ color: "var(--muted)" }}></th>
            </tr>
          </thead>
          <tbody>
            {(printers ?? []).map((p) => (
              <PrinterRow
                key={p.id}
                printer={p}
                isDefault={p.id === currentDefaultId}
                onEdit={() => setEditing(p)}
                onDeleted={onDeleted}
                onSetDefault={(id) => setDefault.mutate({ key: DEFAULT_PRINTER_KEY, value: id })}
              />
            ))}
            <tr style={{ borderTop: "1px solid var(--border)" }}>
              <td className={td} colSpan={2} style={{ color: "var(--muted)" }}>No default printer</td>
              <td className={td}>
                <input
                  type="radio"
                  name="default-printer"
                  aria-label="no default printer"
                  checked={currentDefaultId === null}
                  onChange={() => clearDefault.mutate(DEFAULT_PRINTER_KEY)}
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
