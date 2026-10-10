import type { Dispatch, SetStateAction } from "react";
import { usePrinters } from "../../api/queries";
import type { ParamValue, TemplateDetail } from "../../api/types";
import { ParamInput } from "../../components/ParamInput";
import { getOwnKey } from "../../lib/labelInputs";

export type FormValue = {
  data: Record<string, ParamValue>;
  option?: Record<string, string>;
  printer?: string;
  startSlot: number;
};

const inputClass =
  "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = {
  background: "var(--surface)",
  borderColor: "var(--border)",
  color: "var(--ink)",
} as const;

export function FieldForm({
  detail,
  value,
  onChange,
}: {
  detail: TemplateDetail;
  value: FormValue;
  onChange: Dispatch<SetStateAction<FormValue>>;
}) {
  const { data: printers } = usePrinters();
  const allPrinters = printers ?? [];

  // Every update is applied to the latest state rather than to this render's snapshot: an image read
  // resolves after the render that started it, and an edit made meanwhile must not be undone.
  const setData = (field: string, v: ParamValue) =>
    onChange((prev) => ({ ...prev, data: { ...prev.data, [field]: v } }));

  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;
  const clampSlot = (raw: string) =>
    Math.max(0, Math.min(positions - 1, Math.floor(Number(raw) || 0)));

  return (
    <div className="flex flex-col gap-4">
      {detail.params.map((param) => (
        // Keyed by template too, so a control (and an image read it started) never outlives its template.
        <div key={`${detail.id}:${param.name}`} className="flex flex-col gap-1">
          <div className="flex items-baseline justify-between">
            <span className="text-sm font-medium">{param.description || param.name}</span>
            {param.description && param.description !== param.name && (
              <span className="font-mono text-xs" style={{ color: "var(--muted)" }}>
                {param.name}
              </span>
            )}
          </div>
          {param.control !== "checkbox" && param.default !== undefined && (
            <span className="text-xs" style={{ color: "var(--muted)" }}>{`default: ${String(param.default)}`}</span>
          )}
          <ParamInput
            name={param.name}
            spec={param}
            value={getOwnKey(value.data, param.name)}
            onChange={(v) => setData(param.name, v)}
          />
        </div>
      ))}

      <label className="flex flex-col gap-1">
        <span className="text-sm font-medium">Printer</span>
        <select
          aria-label="printer"
          value={value.printer ?? ""}
          // "" is stored as an EXPLICIT None (distinct from undefined = untouched); PrintForm derives the effective printer.
          onChange={(e) => onChange((prev) => ({ ...prev, printer: e.target.value }))}
          className={inputClass}
          style={inputStyle}
        >
          <option value="">— none (download only) —</option>
          {allPrinters.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>

      {detail.format.type === "sheet" && (
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Start slot</span>
          <input
            type="number"
            min={0}
            max={Math.max(0, positions - 1)}
            aria-label="start slot"
            value={value.startSlot}
            onChange={(e) => onChange((prev) => ({ ...prev, startSlot: clampSlot(e.target.value) }))}
            className={inputClass}
            style={inputStyle}
          />
        </label>
      )}
    </div>
  );
}
