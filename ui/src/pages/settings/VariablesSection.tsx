import { useState } from "react";
import { useVariables, useUpsertVariable } from "../../api/queries";
import { useToast } from "../../app/toast-context";

const SUGGESTED_KEYS = ["qr_base_url"]; // not auto-seeded by the store; shown so the user can fill them
const KEY_RE = /^[A-Za-z0-9_.-]+$/; // mirrors the server's accepted setting-key charset

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function SettingRow({ settingKey, value, suggested }: { settingKey: string; value: string; suggested: boolean }) {
  const [draft, setDraft] = useState(value);
  const upsert = useUpsertVariable();
  const { push } = useToast();
  const dirty = draft !== value;
  return (
    <div className="flex items-end gap-3">
      <label className="flex flex-1 flex-col gap-1">
        <span className="font-mono text-sm font-medium">
          {settingKey}
          {suggested && <span className="ml-2 text-xs" style={{ color: "var(--muted)" }}>(suggested)</span>}
        </span>
        <input
          aria-label={settingKey}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          className={inputClass}
          style={inputStyle}
        />
      </label>
      <button
        type="button"
        aria-label={`save ${settingKey}`}
        disabled={!dirty || upsert.isPending}
        onClick={() =>
          upsert.mutate(
            { key: settingKey, value: draft },
            {
              onSuccess: () => push({ kind: "ok", message: `Saved ${settingKey}` }),
              onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Save failed" }),
            },
          )
        }
        className={`${buttonBase} border`}
        style={{ borderColor: "var(--border)", color: "var(--ink)" }}
      >
        Save
      </button>
    </div>
  );
}

export function VariablesSection() {
  const { data: settings, isError } = useVariables();
  const upsert = useUpsertVariable();
  const { push } = useToast();
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [keyError, setKeyError] = useState<string | null>(null);

  // Render rows only once settings have loaded: SettingRow seeds its draft from `value` on mount and is
  // keyed by the stable setting key, so mounting a row before the real value arrives would strand it at "".
  if (settings === undefined) {
    return (
      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">Variables</h2>
        <p className="text-sm" style={{ color: isError ? "var(--bad)" : "var(--muted)" }}>
          {isError ? "Failed to load settings." : "Loading settings..."}
        </p>
      </section>
    );
  }

  const stored = settings;
  const keys = [...new Set([...Object.keys(stored), ...SUGGESTED_KEYS])].sort();

  const addSetting = () => {
    if (!KEY_RE.test(newKey)) {
      setKeyError("key must be non-empty and contain only letters, digits, '_', '-' or '.'");
      return;
    }
    if (keys.includes(newKey)) {
      // Adding an already-displayed key (including a suggested one) would strand its existing row, so
      // route the user to that row instead of creating a second, out-of-sync row.
      setKeyError("setting already exists; edit its row above");
      return;
    }
    setKeyError(null);
    upsert.mutate(
      { key: newKey, value: newValue },
      {
        onSuccess: () => {
          push({ kind: "ok", message: `Saved ${newKey}` });
          setNewKey("");
          setNewValue("");
        },
        onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Save failed" }),
      },
    );
  };

  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-semibold">Variables</h2>
      <div className="flex flex-col gap-3">
        {keys.map((key) => (
          <SettingRow key={key} settingKey={key} value={stored[key] ?? ""} suggested={!(key in stored)} />
        ))}
      </div>

      <div className="flex flex-col gap-2 border-t pt-4" style={{ borderColor: "var(--border)" }}>
        <span className="text-sm font-medium">Add a setting</span>
        <div className="flex items-end gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-xs" style={{ color: "var(--muted)" }}>new setting key</span>
            <input aria-label="new setting key" value={newKey} onChange={(e) => setNewKey(e.target.value)} className={inputClass} style={inputStyle} />
          </label>
          <label className="flex flex-1 flex-col gap-1">
            <span className="text-xs" style={{ color: "var(--muted)" }}>new setting value</span>
            <input aria-label="new setting value" value={newValue} onChange={(e) => setNewValue(e.target.value)} className={inputClass} style={inputStyle} />
          </label>
          <button type="button" onClick={addSetting} disabled={upsert.isPending} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
            Add setting
          </button>
        </div>
        {keyError && <p className="text-sm" style={{ color: "var(--bad)" }}>{keyError}</p>}
      </div>
    </section>
  );
}
