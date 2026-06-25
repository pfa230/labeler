import { useState, useEffect, useRef } from "react";
import { useSettings, useUpdateSetting, useResetSetting, previewDatetimeFormat } from "../../api/queries";
import { useToast } from "../../app/toast-context";

const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

const KEY = "datetime_formats";
const DEBOUNCE_MS = 400;

interface Row {
  name: string;
  pattern: string;
}

function useDebounce(value: string, delay: number): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(id);
  }, [value, delay]);
  return debounced;
}

interface PreviewState {
  sample: string | null;
  error: string | null;
}

function PatternRow({
  row,
  onChange,
  onRemove,
}: {
  row: Row;
  onChange: (r: Row) => void;
  onRemove: () => void;
}) {
  const debouncedPattern = useDebounce(row.pattern, DEBOUNCE_MS);
  const [preview, setPreview] = useState<PreviewState>({ sample: null, error: null });
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    abortRef.current?.abort();

    if (!debouncedPattern) {
      // schedule the clear asynchronously so we never call setState synchronously in an effect
      const ac = new AbortController();
      abortRef.current = ac;
      Promise.resolve().then(() => {
        if (!ac.signal.aborted) setPreview({ sample: null, error: null });
      });
      return () => ac.abort();
    }

    const ac = new AbortController();
    abortRef.current = ac;

    previewDatetimeFormat(debouncedPattern)
      .then((r) => {
        if (!ac.signal.aborted) setPreview({ sample: r.sample, error: null });
      })
      .catch((err: unknown) => {
        if (!ac.signal.aborted) {
          setPreview({ sample: null, error: err instanceof Error ? err.message : "Invalid pattern" });
        }
      });

    return () => ac.abort();
  }, [debouncedPattern]);

  return (
    <div className="flex items-start gap-2">
      <div className="flex flex-col gap-1">
        <input
          aria-label="Format name"
          placeholder="name"
          value={row.name}
          onChange={(e) => onChange({ ...row, name: e.target.value })}
          className={inputClass}
          style={inputStyle}
        />
      </div>
      <div className="flex flex-col gap-1 flex-1">
        <input
          aria-label="strftime pattern"
          placeholder="%Y-%m-%d"
          value={row.pattern}
          onChange={(e) => onChange({ ...row, pattern: e.target.value })}
          className={`${inputClass} w-full`}
          style={inputStyle}
        />
        {preview.sample !== null && (
          <span className="text-xs" style={{ color: "var(--muted)" }}>
            {preview.sample}
          </span>
        )}
        {preview.error !== null && (
          <span className="text-xs" style={{ color: "var(--bad)" }}>
            {preview.error}
          </span>
        )}
      </div>
      <button
        type="button"
        aria-label="Remove row"
        onClick={onRemove}
        className={`${buttonBase} border mt-0`}
        style={{ borderColor: "var(--border)", color: "var(--muted)" }}
      >
        Remove
      </button>
    </div>
  );
}

function mapToRows(m: Record<string, string>): Row[] {
  return Object.entries(m).map(([name, pattern]) => ({ name, pattern }));
}

function rowsToMap(rows: Row[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const { name, pattern } of rows) {
    if (name) out[name] = pattern;
  }
  return out;
}

export function DatetimeFormatsSection() {
  const { data: settings, isError } = useSettings();
  const update = useUpdateSetting();
  const reset = useResetSetting();
  const { push } = useToast();

  const resolved = settings?.[KEY];
  const serverMap = resolved ? (resolved.value as Record<string, string>) : undefined;

  const [rows, setRows] = useState<Row[] | null>(null);

  // Sync rows from server when settings first loads (or after reset).
  const prevMapRef = useRef<Record<string, string> | undefined>(undefined);
  useEffect(() => {
    if (serverMap === undefined) return;
    // Only reset local state when the server map changes identity (save/reset response).
    if (serverMap !== prevMapRef.current) {
      prevMapRef.current = serverMap;
      setRows(mapToRows(serverMap));
    }
  }, [serverMap]);

  if (settings === undefined) {
    return (
      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">Datetime formats</h2>
        <p className="text-sm" style={{ color: isError ? "var(--bad)" : "var(--muted)" }}>
          {isError ? "Failed to load settings." : "Loading settings..."}
        </p>
      </section>
    );
  }

  if (resolved === undefined) {
    return (
      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">Datetime formats</h2>
        <p className="text-sm" style={{ color: "var(--bad)" }}>Setting unavailable.</p>
      </section>
    );
  }

  const activeRows = rows ?? mapToRows(serverMap ?? {});

  const handleChange = (index: number, updated: Row) => {
    setRows((prev) => {
      const next = [...(prev ?? mapToRows(serverMap ?? {}))];
      next[index] = updated;
      return next;
    });
  };

  const handleRemove = (index: number) => {
    setRows((prev) => {
      const next = [...(prev ?? mapToRows(serverMap ?? {}))];
      next.splice(index, 1);
      return next;
    });
  };

  const handleAdd = () => {
    setRows((prev) => [...(prev ?? mapToRows(serverMap ?? {})), { name: "", pattern: "" }]);
  };

  const handleSave = () => {
    update.mutate(
      { key: KEY, value: rowsToMap(activeRows) },
      {
        onSuccess: () => {
          push({ kind: "ok", message: "Saved" });
        },
        onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Save failed" }),
      },
    );
  };

  const handleReset = () => {
    reset.mutate(KEY, {
      onSuccess: () => {
        setRows(null);
        push({ kind: "ok", message: "Reset to default" });
      },
      onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Reset failed" }),
    });
  };

  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-semibold">
        Datetime formats
        {resolved.is_default && (
          <span className="ml-2 text-xs font-normal" style={{ color: "var(--muted)" }}>(default)</span>
        )}
      </h2>
      <div className="flex flex-col gap-2">
        {activeRows.map((row, i) => (
          // index key is fine: rows are not reorderable; removing a row remounts PatternRow and resets its preview state
          <PatternRow
            key={i}
            row={row}
            onChange={(updated) => handleChange(i, updated)}
            onRemove={() => handleRemove(i)}
          />
        ))}
        <button
          type="button"
          onClick={handleAdd}
          className={`${buttonBase} border self-start`}
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          Add format
        </button>
      </div>
      <div className="flex gap-3">
        <button
          type="button"
          disabled={update.isPending}
          onClick={handleSave}
          className={`${buttonBase} border`}
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          Save
        </button>
        {!resolved.is_default && (
          <button
            type="button"
            disabled={reset.isPending}
            onClick={handleReset}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: "var(--muted)" }}
          >
            Reset to default
          </button>
        )}
      </div>
      <p className="text-xs" style={{ color: "var(--muted)" }}>
        Named strftime patterns available as {"{datetime.<name>}"} in templates.
      </p>
    </section>
  );
}
