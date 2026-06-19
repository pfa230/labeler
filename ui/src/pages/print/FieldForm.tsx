import { usePrinters } from "../../api/queries";
import { imageFields, referencedFields } from "../../lib/templateFields";
import type { TemplateDetail } from "../../api/types";

export type FormValue = {
  data: Record<string, string>;
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

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

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
  const { data: printers } = usePrinters();
  const enabledPrinters = (printers ?? []).filter((p) => p.enabled);

  const setData = (field: string, v: string) =>
    onChange({ ...value, data: { ...value.data, [field]: v } });
  const setOption = (name: string, v: string) =>
    onChange({ ...value, option: { ...value.option, [name]: v } });

  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;
  const clampSlot = (raw: string) =>
    Math.max(0, Math.min(positions - 1, Math.floor(Number(raw) || 0)));

  return (
    <div className="flex flex-col gap-4">
      {fields.map((field) => {
        const current = value.data[field] ?? "";
        const invalid = current.length === 0;
        return (
          <label key={field} className="flex flex-col gap-1">
            <span className="text-sm font-medium">{field}</span>
            {imgs.has(field) ? (
              <>
                <input
                  type="file"
                  accept="image/*"
                  aria-label={field}
                  aria-invalid={invalid}
                  onChange={async (e) => {
                    const file = e.target.files?.[0];
                    if (file) setData(field, await readFileAsDataUrl(file));
                  }}
                  className="text-sm"
                />
                {current && (
                  <span className="text-xs" style={{ color: "var(--muted)" }}>
                    image selected
                  </span>
                )}
              </>
            ) : (
              <input
                type="text"
                aria-label={field}
                aria-invalid={invalid}
                value={current}
                onChange={(e) => setData(field, e.target.value)}
                className={inputClass}
                style={inputStyle}
              />
            )}
          </label>
        );
      })}

      {Object.entries(detail.options ?? {}).map(([name, values]) => (
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
          onChange={(e) => onChange({ ...value, printer: e.target.value || undefined })}
          className={inputClass}
          style={inputStyle}
        >
          <option value="">— none (download only) —</option>
          {enabledPrinters.map((p) => (
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
