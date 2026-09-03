import { useEffect, useLayoutEffect, useRef } from "react";
import type { InputSpec, ParamSpec, ParamValue } from "../api/types";

export interface ParamInputProps {
  name: string;
  spec: InputSpec | ParamSpec;
  value: ParamValue | undefined;
  onChange: (value: ParamValue) => void;
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
  const fileInputRef = useRef<HTMLInputElement>(null);
  // A file chooser is the browser's own state, not the form's: clearing the value it stands for
  // leaves the filename on screen unless the element is cleared too.
  const disabledRef = useRef(disabled);

  useEffect(() => {
    disabledRef.current = disabled;
    if ((!value || disabled) && fileInputRef.current) {
      fileInputRef.current.value = "";
    }
  }, [value, disabled]);

  const pendingFocusRef = useRef<
    | { type: "move-earlier" | "move-later" | "remove"; index: number }
    | { type: "append" }
    | null
  >(null);
  const moveEarlierRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const moveLaterRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const removeRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const appendRef = useRef<HTMLButtonElement | null>(null);

  useLayoutEffect(() => {
    if (!pendingFocusRef.current) return;
    const target = pendingFocusRef.current;
    pendingFocusRef.current = null;

    if (target.type === "append") {
      appendRef.current?.focus();
    } else if (target.type === "move-earlier") {
      moveEarlierRefs.current[target.index]?.focus();
    } else if (target.type === "move-later") {
      moveLaterRefs.current[target.index]?.focus();
    } else if (target.type === "remove") {
      removeRefs.current[target.index]?.focus();
    }
  });

  const label = spec.description || name;
  const inputSpec = spec as InputSpec;
  const paramSpec = spec as ParamSpec;
  const control = inputSpec.control;

  if (control === "image" || isImage) {
    const current = typeof value === "string" ? value : "";
    return (
      <div className="flex flex-col gap-1">
        <input
          ref={fileInputRef}
          type="file"
          accept="image/*"
          aria-label={label}
          aria-invalid={invalid}
          aria-describedby={noteId}
          disabled={disabled}
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            const dataUrl = await readFileAsDataUrl(file);
            // The read finishes after the render that started it. If the entry was deferred again
            // meanwhile, or the chooser no longer holds this file, the value it stood for is gone
            // and must not come back.
            if (disabledRef.current || fileInputRef.current?.files?.[0] !== file) return;
            onChange(dataUrl);
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

  if (control === "textarea" || (paramSpec.type === "string" && paramSpec.multiline)) {
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

  if (
    control === "number" ||
    control === "integer" ||
    inputSpec.slider === true ||
    paramSpec.type === "length" ||
    paramSpec.type === "number" ||
    paramSpec.type === "integer"
  ) {
    const isInteger = control === "integer" || paramSpec.type === "integer";
    const hasDefault = spec.default !== undefined && spec.default !== null;
    const isSlider =
      (inputSpec.slider === true ||
        (!control && spec.min !== undefined && spec.max !== undefined)) &&
      hasDefault;

    if (isSlider) {
      const currentVal =
        value !== undefined && value !== ""
          ? Number(value)
          : Number((spec.default as number | undefined) ?? spec.min ?? 0);

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
            {(inputSpec.unit || (paramSpec.type === "length" && unit)) ? ` ${inputSpec.unit || unit}` : ""}
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

  if (control === "checkbox" || paramSpec.type === "boolean") {
    const isChecked = value !== undefined ? Boolean(value) : undefined;
    return (
      <label className="inline-flex cursor-pointer items-center gap-2">
        <input
          type="checkbox"
          aria-label={label}
          aria-invalid={invalid}
          aria-describedby={noteId}
          disabled={disabled}
          checked={isChecked ?? false}
          onChange={(e) => onChange(e.target.checked)}
          className="h-4 w-4 rounded border"
          style={{ accentColor: "var(--accent)" }}
        />
        <span className="select-none text-sm" style={{ color: isChecked === undefined ? "var(--muted, #888)" : "var(--ink)" }}>
          {isChecked === undefined ? "Unset" : isChecked ? "Enabled" : "Disabled"}
        </span>
      </label>
    );
  }

  if (control === "select" || paramSpec.type === "enum") {
    const currentVal = value !== undefined ? String(value) : "";
    const hasMatch = (spec.values ?? []).includes(currentVal);
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
        {!hasMatch && (
          <option value="" disabled hidden>
            Select...
          </option>
        )}
        {(spec.values ?? []).map((v: string) => (
          <option key={v} value={v}>
            {v}
          </option>
        ))}
      </select>
    );
  }

  if (control === "date" || control === "datetime" || paramSpec.type === "datetime") {
    const inputType =
      control === "datetime" || (control === undefined && paramSpec.time) ? "datetime-local" : "date";
    return (
      <input
        type={inputType}
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

  if (control === "list" || paramSpec.type === "list") {
    const items: string[] = Array.isArray(value) ? (value as string[]) : [];

    return (
      <div
        role="group"
        aria-label={label}
        aria-describedby={noteId}
        className="flex flex-col gap-2"
      >
        {items.map((item, idx) => {
          const pos = idx + 1;
          const isFirst = idx === 0;
          const isLast = idx === items.length - 1;
          const isEarlierInert = !disabled && isFirst;
          const isLaterInert = !disabled && isLast;

          return (
            <div key={idx} className="flex items-center gap-2">
              <input
                type="text"
                aria-label={`${name} ${pos}`}
                aria-invalid={invalid}
                disabled={disabled}
                value={item}
                onChange={(e) => {
                  const next = [...items];
                  next[idx] = e.target.value;
                  onChange(next);
                }}
                className={inputClass}
                style={inputStyle}
              />
              <button
                ref={(el) => {
                  moveEarlierRefs.current[idx] = el;
                }}
                type="button"
                aria-label={`move ${name} ${pos} earlier`}
                aria-disabled={isEarlierInert ? "true" : undefined}
                disabled={disabled}
                onClick={() => {
                  if (isEarlierInert || isFirst) return;
                  pendingFocusRef.current = { type: "move-earlier", index: idx - 1 };
                  const next = [...items];
                  const tmp = next[idx];
                  next[idx] = next[idx - 1];
                  next[idx - 1] = tmp;
                  onChange(next);
                }}
                className="rounded-md border p-1.5 text-xs disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2"
                style={{
                  borderColor: "var(--border)",
                  color: "var(--ink)",
                  opacity: isEarlierInert ? 0.4 : undefined,
                }}
              >
                ↑
              </button>
              <button
                ref={(el) => {
                  moveLaterRefs.current[idx] = el;
                }}
                type="button"
                aria-label={`move ${name} ${pos} later`}
                aria-disabled={isLaterInert ? "true" : undefined}
                disabled={disabled}
                onClick={() => {
                  if (isLaterInert || isLast) return;
                  pendingFocusRef.current = { type: "move-later", index: idx + 1 };
                  const next = [...items];
                  const tmp = next[idx];
                  next[idx] = next[idx + 1];
                  next[idx + 1] = tmp;
                  onChange(next);
                }}
                className="rounded-md border p-1.5 text-xs disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2"
                style={{
                  borderColor: "var(--border)",
                  color: "var(--ink)",
                  opacity: isLaterInert ? 0.4 : undefined,
                }}
              >
                ↓
              </button>
              <button
                ref={(el) => {
                  removeRefs.current[idx] = el;
                }}
                type="button"
                aria-label={`remove ${name} ${pos}`}
                disabled={disabled}
                onClick={() => {
                  if (items.length === 1) {
                    pendingFocusRef.current = { type: "append" };
                  } else if (idx === items.length - 1) {
                    pendingFocusRef.current = { type: "remove", index: idx - 1 };
                  } else {
                    pendingFocusRef.current = { type: "remove", index: idx };
                  }
                  const next = items.filter((_, i) => i !== idx);
                  onChange(next);
                }}
                className="rounded-md border p-1.5 text-xs disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2"
                style={{ borderColor: "var(--border)", color: "var(--ink)" }}
              >
                ✕
              </button>
            </div>
          );
        })}
        <button
          ref={appendRef}
          type="button"
          aria-label={`add ${name}`}
          disabled={disabled}
          onClick={() => {
            onChange([...items, ""]);
          }}
          className="self-start rounded-md border px-3 py-1.5 text-xs font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2"
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          + Add
        </button>
      </div>
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
