import type { ParamSpec, ParamValue } from "../api/types";

export interface ParamInputProps {
  name: string;
  spec: ParamSpec;
  value: ParamValue | undefined;
  onChange: (value: any) => void;
  disabled?: boolean;
  invalid?: boolean;
  isImage?: boolean;
  unit?: string;
  noteId?: string;
}

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

export function ParamInput({
  name,
  spec,
  value,
  onChange,
  disabled = false,
  invalid = false,
  isImage = false,
  unit,
  noteId,
}: ParamInputProps) {
  const label = spec.description || name;

  if (spec.type === "string") {
    if (isImage) {
      const current = typeof value === "string" ? value : "";
      return (
        <div className="flex flex-col gap-1">
          <input
            type="file"
            accept="image/*"
            aria-label={label}
            aria-invalid={invalid}
            aria-describedby={noteId}
            disabled={disabled}
            onChange={async (e) => {
              const file = e.target.files?.[0];
              if (file) onChange(await readFileAsDataUrl(file));
            }}
            className="text-sm"
          />
          {current && (
            <span className="text-xs" style={{ color: "var(--muted)" }}>
              image selected
            </span>
          )}
        </div>
      );
    }

    if (spec.multiline) {
      return (
        <textarea
          aria-label={label}
          aria-invalid={invalid}
          aria-describedby={noteId}
          rows={3}
          disabled={disabled}
          value={value !== undefined ? String(value) : ""}
          onChange={(e) => onChange(e.target.value)}
          className={`${inputClass} resize-y`}
          style={inputStyle}
        />
      );
    }

    return (
      <input
        type="text"
        aria-label={label}
        aria-invalid={invalid}
        aria-describedby={noteId}
        disabled={disabled}
        value={value !== undefined ? String(value) : ""}
        onChange={(e) => onChange(e.target.value)}
        className={inputClass}
        style={inputStyle}
      />
    );
  }

  if (spec.type === "length" || spec.type === "number" || spec.type === "integer") {
    const isSlider = spec.min !== undefined && spec.max !== undefined;
    const isInteger = spec.type === "integer";

    if (isSlider) {
      const currentVal =
        value !== undefined && value !== ""
          ? Number(value)
          : spec.default !== undefined
            ? Number(spec.default)
            : (spec.min ?? 0);

      return (
        <div className="flex items-center gap-3">
          <input
            type="range"
            aria-label={label}
            aria-invalid={invalid}
            aria-describedby={noteId}
            min={spec.min}
            max={spec.max}
            step={isInteger ? 1 : "any"}
            disabled={disabled}
            value={currentVal}
            onChange={(e) => {
              const raw = e.target.value;
              const parsed = isInteger ? parseInt(raw, 10) : parseFloat(raw);
              onChange(Number.isNaN(parsed) ? "" : parsed);
            }}
            className="w-full accent-[var(--accent)]"
          />
          <span className="min-w-12 text-right font-mono text-sm" style={{ color: "var(--ink)" }}>
            {currentVal}
            {spec.type === "length" && unit ? ` ${unit}` : ""}
          </span>
        </div>
      );
    }

    return (
      <input
        type="number"
        aria-label={label}
        aria-invalid={invalid}
        aria-describedby={noteId}
        min={spec.min}
        max={spec.max}
        step={isInteger ? 1 : "any"}
        disabled={disabled}
        value={typeof value === "number" || typeof value === "string" ? value : ""}
        onChange={(e) => {
          const raw = e.target.value;
          if (raw === "") {
            onChange("");
          } else {
            const parsed = isInteger ? parseInt(raw, 10) : parseFloat(raw);
            onChange(Number.isNaN(parsed) ? "" : parsed);
          }
        }}
        className={inputClass}
        style={inputStyle}
      />
    );
  }

  if (spec.type === "boolean") {
    const isChecked = Boolean(
      value !== undefined ? value : (spec.default ?? false),
    );
    return (
      <label className="inline-flex cursor-pointer items-center gap-2">
        <input
          type="checkbox"
          aria-label={label}
          aria-invalid={invalid}
          aria-describedby={noteId}
          disabled={disabled}
          checked={isChecked}
          onChange={(e) => onChange(e.target.checked)}
          className="h-4 w-4 rounded border"
          style={{ accentColor: "var(--accent)" }}
        />
        <span className="select-none text-sm" style={{ color: "var(--ink)" }}>
          {isChecked ? "Enabled" : "Disabled"}
        </span>
      </label>
    );
  }

  if (spec.type === "enum") {
    const currentVal = String(
      value !== undefined ? value : (spec.default ?? spec.values?.[0] ?? ""),
    );
    return (
      <select
        aria-label={label}
        aria-invalid={invalid}
        aria-describedby={noteId}
        disabled={disabled}
        value={currentVal}
        onChange={(e) => onChange(e.target.value)}
        className={inputClass}
        style={inputStyle}
      >
        {(spec.values ?? []).map((v) => (
          <option key={v} value={v}>
            {v}
          </option>
        ))}
      </select>
    );
  }

  return (
    <input
      type="text"
      aria-label={label}
      aria-invalid={invalid}
      aria-describedby={noteId}
      disabled={disabled}
      value={value !== undefined ? String(value) : ""}
      onChange={(e) => onChange(e.target.value)}
      className={inputClass}
      style={inputStyle}
    />
  );
}
