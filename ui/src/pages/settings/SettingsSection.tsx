import { useState } from "react";
import { useSettings, useUpdateSetting, useResetSetting } from "../../api/queries";
import { useToast } from "../../app/toast-context";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

const KEY = "job_log_retention_days";

export function SettingsSection() {
  const { data: settings, isError } = useSettings();
  const update = useUpdateSetting();
  const reset = useResetSetting();
  const { push } = useToast();
  const [draft, setDraft] = useState<string | null>(null);

  if (settings === undefined) {
    return (
      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">Job log</h2>
        <p className="text-sm" style={{ color: isError ? "var(--bad)" : "var(--muted)" }}>
          {isError ? "Failed to load settings." : "Loading settings..."}
        </p>
      </section>
    );
  }

  const resolved = settings[KEY];

  if (resolved === undefined) {
    return (
      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">Job log</h2>
        <p className="text-sm" style={{ color: "var(--bad)" }}>Setting unavailable.</p>
      </section>
    );
  }

  const current = draft ?? String(resolved.value ?? "");
  const dirty = draft !== null && draft !== String(resolved.value ?? "");

  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-semibold">Job log</h2>
      <div className="flex items-end gap-3">
        <label className="flex flex-col gap-1">
          <span className="font-mono text-sm font-medium">
            {KEY}
            {resolved.is_default && (
              <span className="ml-2 text-xs" style={{ color: "var(--muted)" }}>(default)</span>
            )}
          </span>
          <input
            type="number"
            min={0}
            aria-label={KEY}
            value={current}
            onChange={(e) => setDraft(e.target.value)}
            className={inputClass}
            style={inputStyle}
          />
        </label>
        <button
          type="button"
          disabled={!dirty || update.isPending}
          onClick={() =>
            update.mutate(
              { key: KEY, value: Math.max(0, Math.floor(Number(current) || 0)) },
              {
                onSuccess: () => { setDraft(null); push({ kind: "ok", message: "Saved" }); },
                onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Save failed" }),
              },
            )
          }
          className={`${buttonBase} border`}
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          Save
        </button>
        {!resolved.is_default && (
          <button
            type="button"
            disabled={reset.isPending}
            onClick={() =>
              reset.mutate(KEY, {
                onSuccess: () => { setDraft(null); push({ kind: "ok", message: "Reset to default" }); },
                onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Reset failed" }),
              })
            }
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: "var(--muted)" }}
          >
            Reset to default
          </button>
        )}
      </div>
      <p className="text-xs" style={{ color: "var(--muted)" }}>
        Days of job history to keep. 0 disables pruning.
      </p>
    </section>
  );
}
