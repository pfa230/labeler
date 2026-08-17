import { usePrinters } from "../../api/queries";
import {
  imageFields,
  multilineFields,
  referencedFields,
  singleLineTextFields,
} from "../../lib/templateFields";
import type { ParamSpec, ParamValue, TemplateDetail } from "../../api/types";
import { ParamInput } from "../../components/ParamInput";

export type FormValue = {
  data: Record<string, ParamValue>;
  option: Record<string, string>;
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
  onChange: (v: FormValue) => void;
}) {
  const fields = referencedFields(detail.layout, value.option);
  const imgs = new Set(imageFields(detail.layout, value.option));
  const multiline = new Set(multilineFields(detail.layout, value.option));
  // Ungated on both sides: `value.data` survives an option switch, so a value typed where the field
  // is multiline is submitted where it may not be.
  const singleLineAnywhere = new Set(singleLineTextFields(detail.layout, {}));
  const truncatedSomewhere = new Set(
    multilineFields(detail.layout, {}).filter((f) => singleLineAnywhere.has(f)),
  );
  const { data: printers } = usePrinters();
  const allPrinters = printers ?? [];

  const setData = (field: string, v: ParamValue) =>
    onChange({ ...value, data: { ...value.data, [field]: v } });
  const setOption = (name: string, v: string) =>
    onChange({ ...value, option: { ...value.option, [name]: v } });

  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;
  const clampSlot = (raw: string) =>
    Math.max(0, Math.min(positions - 1, Math.floor(Number(raw) || 0)));

  const declaredParams = detail.params ?? {};
  const hasDeclaredParams = Object.keys(declaredParams).length > 0;

  // Unhandled fields from layout (for backward compatibility when params is missing or partial)
  const fallbackFields = fields.filter((f) => !declaredParams[f]);
  const fallbackOptions = Object.entries(detail.options ?? {}).filter(
    ([name]) => !declaredParams[name],
  );

  return (
    <div className="flex flex-col gap-4">
      {hasDeclaredParams &&
        Object.entries(declaredParams).map(([name, spec], i) => {
          const current = value.data[name];
          const hasDefault =
            spec.default !== undefined ||
            spec.type === "boolean" ||
            (spec.type === "enum" && (spec.values?.length ?? 0) > 0);
          const invalid = !hasDefault && (current === undefined || current === "");
          const noteId = truncatedSomewhere.has(name) ? `multiline-note-${i}` : undefined;

          return (
            <label key={name} className="flex flex-col gap-1">
              <div className="flex items-baseline justify-between">
                <span className="text-sm font-medium">{spec.description || name}</span>
                {spec.description && spec.description !== name && (
                  <span className="font-mono text-xs" style={{ color: "var(--muted)" }}>
                    {name}
                  </span>
                )}
              </div>
              <ParamInput
                name={name}
                spec={spec}
                value={current}
                onChange={(v) => setData(name, v)}
                isImage={imgs.has(name)}
                unit={detail.unit}
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

      {!hasDeclaredParams &&
        fallbackFields.map((field, i) => {
          const current = value.data[field] !== undefined ? String(value.data[field]) : "";
          const invalid = current.length === 0;
          const noteId = truncatedSomewhere.has(field) ? `multiline-note-${i}` : undefined;
          const spec: ParamSpec = {
            type: "string",
            multiline: multiline.has(field),
          };

          return (
            <label key={field} className="flex flex-col gap-1">
              <span className="text-sm font-medium">{field}</span>
              <ParamInput
                name={field}
                spec={spec}
                value={current}
                onChange={(v) => setData(field, v)}
                isImage={imgs.has(field)}
                unit={detail.unit}
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

      {hasDeclaredParams &&
        fallbackFields.map((field, i) => {
          const current = value.data[field] !== undefined ? String(value.data[field]) : "";
          const invalid = current.length === 0;
          const noteId = truncatedSomewhere.has(field) ? `multiline-note-fb-${i}` : undefined;
          const spec: ParamSpec = {
            type: "string",
            multiline: multiline.has(field),
          };

          return (
            <label key={field} className="flex flex-col gap-1">
              <span className="text-sm font-medium">{field}</span>
              <ParamInput
                name={field}
                spec={spec}
                value={current}
                onChange={(v) => setData(field, v)}
                isImage={imgs.has(field)}
                unit={detail.unit}
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

      {fallbackOptions.map(([name, values]) => (
        <label key={name} className="flex flex-col gap-1">
          <span className="text-sm font-medium">{name}</span>
          <select
            aria-label={name}
            value={value.option[name] ?? values[0] ?? ""}
            onChange={(e) => setOption(name, e.target.value)}
            className={inputClass}
            style={inputStyle}
          >
            {values.map((v) => (
              <option key={v} value={v}>
                {v}
              </option>
            ))}
          </select>
        </label>
      ))}

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
