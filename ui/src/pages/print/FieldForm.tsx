import type { Dispatch, SetStateAction } from "react";
import { usePrinters } from "../../api/queries";
import type { Param, ParamValue, TemplateDetail } from "../../api/types";
import { ParamInput } from "../../components/ParamInput";
import { getOwnKey, seedDefaultValue } from "../../lib/labelInputs";

export type FormValue = {
  data: Record<string, ParamValue>;
  // Deferral is concrete, never inferred: a non-checkbox parameter publishing a default is present and
  // true from the start, so what the form renders and what submission omits read one map.
  deferred: Record<string, boolean>;
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
  // resolves after the render that started it, and re-checking meanwhile must not be undone.
  const setData = (field: string, v: ParamValue) =>
    onChange((prev) => ({ ...prev, data: { ...prev.data, [field]: v } }));

  // Re-checking discards whatever was entered while the checkbox was cleared, putting the control
  // back to what the seeding rule gave it. Clearing leaves that value in place, to be submitted.
  const toggleDeferred = (param: Param, checked: boolean) =>
    onChange((prev) => ({
      ...prev,
      deferred: { ...prev.deferred, [param.name]: checked },
      data: checked ? { ...prev.data, [param.name]: seedDefaultValue(param) } : prev.data,
    }));

  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;
  const clampSlot = (raw: string) =>
    Math.max(0, Math.min(positions - 1, Math.floor(Number(raw) || 0)));

  return (
    <div className="flex flex-col gap-4">
      {detail.params.map((param) => {
        // A checkbox always holds a value, so it has nothing to defer to.
        const defers = param.control !== "checkbox" && param.default !== undefined && param.default !== null;
        const isDeferred = defers && getOwnKey(value.deferred, param.name) === true;

        return (
          <div key={param.name} className="flex flex-col gap-1">
            <div className="flex items-baseline justify-between">
              <span className="text-sm font-medium">{param.description || param.name}</span>
              {param.description && param.description !== param.name && (
                <span className="font-mono text-xs" style={{ color: "var(--muted)" }}>
                  {param.name}
                </span>
              )}
            </div>
            {defers && (
              <label className="inline-flex cursor-pointer items-center gap-1.5 text-xs select-none" style={{ color: "var(--muted)" }}>
                <input
                  type="checkbox"
                  // The accessible name carries the parameter's `name`, which is unique within a
                  // template, so two parameters sharing a description and a default stay
                  // distinguishable. This label is the checkbox's own; the value control never shares it.
                  aria-label={`Use default for ${param.name}`}
                  checked={isDeferred}
                  onChange={(e) => toggleDeferred(param, e.target.checked)}
                  className="h-3.5 w-3.5 rounded border"
                  style={{ accentColor: "var(--accent)" }}
                />
                <span>
                  Use default: <span className="font-mono">{String(param.default)}</span>
                </span>
              </label>
            )}
            <ParamInput
              name={param.name}
              spec={param}
              value={getOwnKey(value.data, param.name)}
              onChange={(v) => setData(param.name, v)}
              disabled={isDeferred}
            />
          </div>
        );
      })}

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
