import { useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import {
  useConnections,
  useSaveConnection,
  useDeleteConnection,
  type Connection,
  type ConnectionCreate,
  type ConnectionUpdate,
} from "../../api/connectors";
import { useToast } from "../../app/toast-context";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

const RETURN_PATHS = ["/connect", "/connections"];

function getReturnPath(locationState: unknown): string {
  const from = (locationState as { from?: unknown } | null)?.from;
  return typeof from === "string" && RETURN_PATHS.includes(from) ? from : "/connections";
}

function CreateConnectionForm() {
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [publicUrl, setPublicUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const save = useSaveConnection();
  const { push } = useToast();
  const location = useLocation();
  const navigate = useNavigate();

  const returnPath = getReturnPath(location.state);

  const submit = () => {
    if (name.trim() === "") { setError("name must not be empty"); return; }
    let url: URL;
    try { url = new URL(baseUrl.trim()); } catch { setError("base url must be a valid URL"); return; }
    if (url.protocol !== "http:" && url.protocol !== "https:") { setError("base url must be http or https"); return; }
    if (publicUrl.trim() !== "") {
      let pubUrl: URL;
      try { pubUrl = new URL(publicUrl.trim()); } catch { setError("public url must be a valid URL"); return; }
      if (pubUrl.protocol !== "http:" && pubUrl.protocol !== "https:") { setError("public url must be http or https"); return; }
    }
    if (apiKey.trim() === "") { setError("api key is required"); return; }
    setError(null);
    const input: ConnectionCreate = {
      connector: "homebox",
      name: name.trim(),
      base_url: baseUrl.trim(),
      ...(publicUrl.trim() !== "" ? { public_url: publicUrl.trim() } : {}),
      credential: apiKey.trim(),
    };
    save.mutate(
      { input },
      {
        onSuccess: () => {
          push({ kind: "ok", message: `Saved ${input.name}` });
          navigate(returnPath);
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Save failed";
          setError(message);
          push({ kind: "error", message });
        },
      },
    );
  };

  return (
    <div className="flex max-w-xl flex-col gap-6">
      <h1 className="text-2xl font-semibold">New connection</h1>

      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">Details</h2>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>connector</span>
          <select aria-label="connector" value="homebox" disabled className={inputClass} style={inputStyle}>
            <option value="homebox">homebox</option>
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>name</span>
          <input aria-label="name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>base url</span>
          <input aria-label="base url" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="http://homebox.lan:7745" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>public url</span>
          <input aria-label="public url" value={publicUrl} onChange={(e) => setPublicUrl(e.target.value)} placeholder="https://homebox.example.com" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>api key</span>
          <input aria-label="api key" type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
      </section>

      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      <div className="flex gap-3 pt-2">
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink)" }}>Save</button>
        <button type="button" onClick={() => navigate(returnPath)} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Cancel</button>
      </div>
    </div>
  );
}

function EditConnectionForm({ initial }: { initial: Connection }) {
  const [name, setName] = useState(initial.name);
  const [baseUrl, setBaseUrl] = useState(initial.base_url);
  const [publicUrl, setPublicUrl] = useState(initial.public_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const save = useSaveConnection();
  const remove = useDeleteConnection();
  const { push } = useToast();
  const location = useLocation();
  const navigate = useNavigate();

  const returnPath = getReturnPath(location.state);

  const submit = () => {
    if (name.trim() === "") { setFormError("name must not be empty"); return; }
    let url: URL;
    try { url = new URL(baseUrl.trim()); } catch { setFormError("base url must be a valid URL"); return; }
    if (url.protocol !== "http:" && url.protocol !== "https:") { setFormError("base url must be http or https"); return; }
    if (publicUrl.trim() !== "") {
      let pubUrl: URL;
      try { pubUrl = new URL(publicUrl.trim()); } catch { setFormError("public url must be a valid URL"); return; }
      if (pubUrl.protocol !== "http:" && pubUrl.protocol !== "https:") { setFormError("public url must be http or https"); return; }
    }
    setFormError(null);

    const input: ConnectionUpdate = {
      name: name.trim(),
      base_url: baseUrl.trim(),
      ...(publicUrl.trim() !== "" ? { public_url: publicUrl.trim() } : {}),
      ...(apiKey.trim() !== "" ? { credential: apiKey.trim() } : {}),
    };

    save.mutate(
      { input, id: initial.id },
      {
        onSuccess: () => {
          push({ kind: "ok", message: `Saved ${input.name}` });
          navigate(returnPath);
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Save failed";
          setFormError(message);
          push({ kind: "error", message });
        },
      },
    );
  };

  const handleDelete = () => {
    remove.mutate(initial.id, {
      onSuccess: () => {
        push({ kind: "ok", message: `Deleted ${initial.name}` });
        navigate("/connections");
      },
      onError: (err) => {
        push({ kind: "error", message: err instanceof Error ? err.message : "Delete failed" });
      },
    });
  };

  return (
    <div className="flex max-w-xl flex-col gap-6">
      <h1 className="text-2xl font-semibold">Edit {initial.name}</h1>

      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">Details</h2>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>connector</span>
          <select aria-label="connector" value={initial.connector} disabled className={inputClass} style={inputStyle}>
            <option value={initial.connector}>{initial.connector}</option>
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>name</span>
          <input aria-label="name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>base url</span>
          <input aria-label="base url" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="http://homebox.lan:7745" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>public url</span>
          <input aria-label="public url" value={publicUrl} onChange={(e) => setPublicUrl(e.target.value)} placeholder="https://homebox.example.com" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>api key (leave blank to keep)</span>
          <input aria-label="api key" type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
      </section>

      {formError && <p className="text-sm" style={{ color: "var(--bad)" }}>{formError}</p>}
      <div className="flex flex-wrap items-center justify-between gap-3 pt-4 border-t" style={{ borderColor: "var(--border)" }}>
        <div className="flex gap-3">
          <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink)" }}>Save</button>
          <button type="button" onClick={() => navigate(returnPath)} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Cancel</button>
        </div>
        <div className="flex items-center gap-2">
          {confirmingDelete ? (
            <>
              <button
                type="button"
                disabled={remove.isPending}
                onClick={handleDelete}
                className={buttonBase}
                style={{ color: "var(--bad)" }}
              >
                Confirm
              </button>
              <button
                type="button"
                onClick={() => setConfirmingDelete(false)}
                className={`${buttonBase} border`}
                style={{ borderColor: "var(--border)", color: "var(--muted)" }}
              >
                Cancel
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={() => setConfirmingDelete(true)}
              className={buttonBase}
              style={{ color: "var(--bad)" }}
            >
              Delete
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

export function ConnectionForm() {
  const { id } = useParams<{ id?: string }>();
  const location = useLocation();
  const { data: connections, isPending, isError } = useConnections();

  const isNew = id === undefined;

  if (!isNew) {
    if (isPending && !connections) {
      return <p className="text-sm" style={{ color: "var(--muted)" }}>Loading connections...</p>;
    }
    if (isError && !connections) {
      return <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load connections.</p>;
    }
    const conn = connections?.find((c) => c.id === id);
    if (!conn) {
      return (
        <div className="flex flex-col gap-4">
          <p className="text-sm" style={{ color: "var(--bad)" }}>Connection &quot;{id}&quot; not found.</p>
          <Link to="/connections" className="text-sm underline" style={{ color: "var(--ink)" }}>
            Back to connections
          </Link>
        </div>
      );
    }

    return (
      <EditConnectionForm
        key={`${location.key}:${id}`}
        initial={conn}
      />
    );
  }

  return (
    <CreateConnectionForm
      key={`${location.key}:new`}
    />
  );
}
