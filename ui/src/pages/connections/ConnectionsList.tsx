import { Link, useLocation } from "react-router-dom";
import { useConnections, useSetDefaultConnection, useClearDefaultConnection } from "../../api/connectors";
import { useSettings } from "../../api/queries";
import { useToast } from "../../app/toast-context";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const th = "px-3 py-2 text-left text-xs font-medium";
const td = "px-3 py-2 text-sm";

export function ConnectionsList() {
  const { data: connections, isPending, isError } = useConnections();
  const { data: settings } = useSettings();
  const setDefaultConn = useSetDefaultConnection();
  const clearDefaultConn = useClearDefaultConnection();
  const { push } = useToast();
  const location = useLocation();

  const rawFrom = (location.state as { from?: unknown } | null)?.from;
  const from = rawFrom !== undefined && rawFrom !== null ? rawFrom : undefined;

  const storedDefault = settings?.default_connection_id;
  const storedDefaultId = typeof storedDefault?.value === "string" ? storedDefault.value : null;
  const isDefault = storedDefault?.is_default ?? true;
  const matchingConn = storedDefaultId ? (connections ?? []).find((c) => c.id === storedDefaultId) : null;
  const connectionsKnown = !isPending && !isError;
  const isDangling = connectionsKnown && storedDefaultId !== null && !matchingConn && !isDefault;

  const handleDefaultChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const val = e.target.value;
    if (!val) {
      clearDefaultConn.mutate(undefined, {
        onSuccess: () => push({ kind: "ok", message: "Default connection reset to default" }),
        onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Failed to clear default connection" }),
      });
    } else {
      setDefaultConn.mutate(val, {
        onSuccess: () => push({ kind: "ok", message: "Default connection saved" }),
        onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Failed to save default connection" }),
      });
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Connections</h1>
        <Link
          to="/connections/new"
          state={from !== undefined ? { from } : undefined}
          className={`${buttonBase} border`}
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          Add connection
        </Link>
      </div>

      {isPending ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>Loading connections...</p>
      ) : isError ? (
        <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load connections.</p>
      ) : (connections ?? []).length === 0 ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>No connections configured.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
            <thead>
              <tr>
                <th className={th} style={{ color: "var(--muted)" }}>Name</th>
                <th className={th} style={{ color: "var(--muted)" }}>Connector</th>
                <th className={th} style={{ color: "var(--muted)" }}>Base URL</th>
                <th className={th} style={{ color: "var(--muted)" }}>Public URL</th>
                <th className={th} style={{ color: "var(--muted)" }}>API key</th>
                <th className={th} style={{ color: "var(--muted)" }}>Enabled</th>
                <th className={th} style={{ color: "var(--muted)" }}></th>
              </tr>
            </thead>
            <tbody>
              {(connections ?? []).map((c) => (
                <tr key={c.id} style={{ borderTop: "1px solid var(--border)" }}>
                  <td className={td}>{c.name}</td>
                  <td className={`${td} font-mono`}>{c.connector}</td>
                  <td className={`${td} font-mono`}>{c.base_url}</td>
                  <td className={`${td} font-mono`}>{c.public_url || "-"}</td>
                  <td className={td}>{c.has_credential ? "set" : "none"}</td>
                  <td className={td}>{c.enabled ? "yes" : "no"}</td>
                  <td className={`${td} flex gap-2`}>
                    <Link
                      to={`/connections/${encodeURIComponent(c.id)}`}
                      state={from !== undefined ? { from } : undefined}
                      className="underline"
                      style={{ color: "var(--ink)" }}
                    >
                      Edit
                    </Link>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div className="flex flex-col gap-1 max-w-md pt-2 border-t" style={{ borderColor: "var(--border)" }}>
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Default connection</span>
          <select
            aria-label="default connection"
            value={isDefault || !storedDefaultId ? "" : storedDefaultId}
            disabled={setDefaultConn.isPending || clearDefaultConn.isPending || !connectionsKnown}
            onChange={handleDefaultChange}
            className={inputClass}
            style={inputStyle}
          >
            <option value="">(no default)</option>
            {isDangling && (
              <option value={storedDefaultId}>
                {storedDefaultId} (unavailable)
              </option>
            )}
            {!connectionsKnown && storedDefaultId !== null && !isDefault && (
              <option value={storedDefaultId}>{storedDefaultId}</option>
            )}
            {(connections ?? []).map((c) => (
              <option key={c.id} value={c.id}>
                {c.name} ({c.id}){c.enabled ? "" : " (disabled)"}
              </option>
            ))}
          </select>
        </label>
        <p className="text-xs" style={{ color: "var(--muted)" }}>
          The default connection applies to everyone on this instance.
        </p>
      </div>
    </div>
  );
}
