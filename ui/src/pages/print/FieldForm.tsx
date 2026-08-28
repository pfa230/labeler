import { usePrinters } from "../../api/queries";
import type { InputSpec, ParamValue, TemplateDetail } from "../../api/types";
import { ParamInput } from "../../components/ParamInput";

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
  inputs,
  value,
  onChange,
}: {
  detail: TemplateDetail;
  inputs?: InputSpec[];
  value: FormValue;
  onChange: (v: FormValue) => void;
}) {
  const activeInputs = inputs ?? detail.inputs?.default ?? [];
  const { data: printers } = usePrinters();
  const allPrinters = printers ?? [];

  const setData = (field: string, v: ParamValue) =>
    onChange({ ...value, data: { ...value.data, [field]: v } });

  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;
  const clampSlot = (raw: string) =>
    Math.max(0, Math.min(positions - 1, Math.floor(Number(raw) || 0)));

  return (
    <div className="flex flex-col gap-4">
      {activeInputs.map((input, i) => {
        const current = value.data[input.name];
        const invalid = input.required && (current === undefined || current === "" || current === null);
        const noteId = input.truncated_elsewhere ? `multiline-note-${i}` : undefined;

        return (
          <label key={input.name} className="flex flex-col gap-1">
            <div className="flex items-baseline justify-between">
              <span className="text-sm font-medium">{input.description || input.name}</span>
              {input.description && input.description !== input.name && (
                <span className="font-mono text-xs" style={{ color: "var(--muted)" }}>
                  {input.name}
                </span>
              )}
            </div>
            <ParamInput
              name={input.name}
              spec={input}
              value={current}
              onChange={(v) => setData(input.name, v)}
              unit={input.unit || detail.unit}
              noteId={noteId}
              invalid={invalid}
            />
            {noteId && (
              <span id={noteId} className="text-xs" style={{ color: "var(--muted)" }}>
                Also used on a single-line item, which shows only the first line.
              </span>
            )}
          </label>
        );
      })}

      <label className="flex flex-col gap-1">
        <span className="text-sm font-medium">Printer</span>
        <select
          aria-label="printer"
          value={value.printer ?? ""}
          // "" is stored as an EXPLICIT None (distinct from undefined = untouched); PrintForm derives the effective printer.
          onChange={(e) => onChange({ ...value, printer: e.target.value })}
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
            onChange={(e) => onChange({ ...value, startSlot: clampSlot(e.target.value) })}
            className={inputClass}
            style={inputStyle}
          />
        </label>
      )}
    </div>
  );
}
