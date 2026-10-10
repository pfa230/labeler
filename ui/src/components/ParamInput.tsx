import { useEffect, useLayoutEffect, useRef } from "react";
import type { Param, ParamValue } from "../api/types";
import { blankOptionLabel } from "../lib/labelInputs";

export interface ParamInputProps {
  name: string;
  spec: Param;
  value: ParamValue | undefined;
  onChange: (value: ParamValue) => void;
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
}: ParamInputProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  // A file chooser is the browser's own state, not the form's: clearing the value it stands for
  // leaves the filename on screen unless the element is cleared too.
  useEffect(() => {
    if (!value && fileInputRef.current) {
      fileInputRef.current.value = "";
    }
  }, [value]);

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
  const control = spec.control;

  if (control === "image") {
    const current = typeof value === "string" ? value : "";
    return (
      <div className="flex flex-col gap-1">
        <input
          ref={fileInputRef}
          type="file"
          accept="image/*"
          aria-label={label}
          onChange={async (e) => {
            const file = e.target.files?.[0];
            // A cancelled selection empties the chooser, so the value it held goes too.
            if (!file) {
              onChange("");
              return;
            }
            const dataUrl = await readFileAsDataUrl(file);
            // The read finishes after the render that started it. If the chooser no longer holds
            // this file, the value it stood for is gone and must not come back.
            if (fileInputRef.current?.files?.[0] !== file) return;
            onChange(dataUrl);
          }}
          className="text-sm"
        />
        {current && (
          <span className="flex items-center gap-2 text-xs" style={{ color: "var(--muted)" }}>
            image selected
            <button
              type="button"
              aria-label={`clear ${label}`}
              onClick={() => onChange("")}
              className="rounded-md border px-2 py-0.5 focus-visible:outline-none focus-visible:ring-2"
              style={{ borderColor: "var(--border)", color: "var(--ink)" }}
            >
              Clear
            </button>
          </span>
        )}
      </div>
    );
  }

  if (control === "textarea") {
    return (
      <textarea
        aria-label={label}
        rows={3}
        value={value !== undefined ? String(value) : ""}
        onChange={(e) => onChange(e.target.value)}
        className={`${inputClass} resize-y`}
        style={inputStyle}
      />
    );
  }

  if (control === "number" || control === "integer") {
    const isInteger = control === "integer";
    return (
      <input
        type="number"
        aria-label={label}
        min={spec.min}
        max={spec.max}
        step={isInteger ? 1 : "any"}
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

  if (control === "checkbox") {
    return (
      <input
        type="checkbox"
        aria-label={label}
        checked={value === undefined ? spec.default === true : value === true}
        onChange={(e) => onChange(e.target.checked)}
        className="h-4 w-4 rounded border"
        style={{ accentColor: "var(--accent)" }}
      />
    );
  }

  if (control === "select") {
    return (
      <select
        aria-label={label}
        value={value !== undefined ? String(value) : ""}
        onChange={(e) => onChange(e.target.value)}
        className={inputClass}
        style={inputStyle}
      >
        <option value="">{blankOptionLabel(spec)}</option>
        {(spec.values ?? []).map((v: string) => (
          <option key={v} value={v}>
            {v}
          </option>
        ))}
      </select>
    );
  }

  if (control === "date" || control === "datetime") {
    const inputType = control === "datetime" ? "datetime-local" : "date";
    return (
      <input
        type={inputType}
        aria-label={label}
        value={value !== undefined ? String(value) : ""}
        onChange={(e) => onChange(e.target.value)}
        className={inputClass}
        style={inputStyle}
      />
    );
  }

  if (control === "list") {
    const items: string[] = Array.isArray(value) ? (value as string[]) : [];

    return (
      <div
        role="group"
        aria-label={label}
        className="flex flex-col gap-2"
      >
        {items.map((item, idx) => {
          const pos = idx + 1;
          const isFirst = idx === 0;
          const isLast = idx === items.length - 1;

          return (
            <div key={idx} className="flex items-center gap-2">
              <input
                type="text"
                aria-label={`${name} ${pos}`}
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
                aria-disabled={isFirst ? "true" : undefined}
                onClick={() => {
                  if (isFirst) return;
                  pendingFocusRef.current = { type: "move-earlier", index: idx - 1 };
                  const next = [...items];
                  const tmp = next[idx];
                  next[idx] = next[idx - 1];
                  next[idx - 1] = tmp;
                  onChange(next);
                }}
                className="rounded-md border p-1.5 text-xs focus-visible:outline-none focus-visible:ring-2"
                style={{
                  borderColor: "var(--border)",
                  color: "var(--ink)",
                  opacity: isFirst ? 0.4 : undefined,
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
                aria-disabled={isLast ? "true" : undefined}
                onClick={() => {
                  if (isLast) return;
                  pendingFocusRef.current = { type: "move-later", index: idx + 1 };
                  const next = [...items];
                  const tmp = next[idx];
                  next[idx] = next[idx + 1];
                  next[idx + 1] = tmp;
                  onChange(next);
                }}
                className="rounded-md border p-1.5 text-xs focus-visible:outline-none focus-visible:ring-2"
                style={{
                  borderColor: "var(--border)",
                  color: "var(--ink)",
                  opacity: isLast ? 0.4 : undefined,
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
                className="rounded-md border p-1.5 text-xs focus-visible:outline-none focus-visible:ring-2"
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
          onClick={() => {
            onChange([...items, ""]);
          }}
          className="self-start rounded-md border px-3 py-1.5 text-xs font-medium focus-visible:outline-none focus-visible:ring-2"
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
      value={value !== undefined ? String(value) : ""}
      onChange={(e) => onChange(e.target.value)}
      className={inputClass}
      style={inputStyle}
    />
  );
}
