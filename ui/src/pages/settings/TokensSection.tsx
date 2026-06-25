import { useState } from "react";
import { useTokens, useCreateToken, useDeleteToken } from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { ApiToken } from "../../api/queries";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function NewSecret({ secret, onDismiss }: { secret: string; onDismiss: () => void }) {
  const { push } = useToast();
  const copy = () => {
    void navigator.clipboard?.writeText(secret);
    push({ kind: "ok", message: "Copied to clipboard" });
  };
  return (
    <div className="flex flex-col gap-2 rounded-md border p-4" style={{ borderColor: "var(--border)", background: "var(--surface)" }}>
      <span className="text-sm font-medium">New token created</span>
      <p className="text-xs" style={{ color: "var(--bad)" }}>Copy it now. You will not see this secret again.</p>
      <div className="flex items-center gap-3">
        <code aria-label="token secret" className="flex-1 break-all rounded-md border px-3 py-2 font-mono text-sm" style={{ borderColor: "var(--border)" }}>
          {secret}
        </code>
        <button type="button" onClick={copy} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
          Copy
        </button>
        <button type="button" onClick={onDismiss} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--muted)" }}>
          Done
        </button>
      </div>
    </div>
  );
}

function CreateTokenForm({ onCreated }: { onCreated: (secret: string) => void }) {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const create = useCreateToken();
  const { push } = useToast();

  const submit = () => {
    if (name.trim() === "") {
      setError("name must not be empty");
      return;
    }
    setError(null);
    create.mutate(
      { name: name.trim() },
      {
        onSuccess: (token) => {
          push({ kind: "ok", message: `Created ${token.name}` });
          setName("");
          onCreated(token.secret);
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Create failed";
          setError(message);
          push({ kind: "error", message });
        },
      },
    );
  };

  return (
    <div className="flex flex-col gap-2 border-t pt-4" style={{ borderColor: "var(--border)" }}>
      <span className="text-sm font-medium">Create a token</span>
      <div className="flex items-end gap-3">
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>token name</span>
          <input aria-label="token name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <button type="button" onClick={submit} disabled={create.isPending} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
          Create token
        </button>
      </div>
      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
    </div>
  );
}

function TokenRow({ token }: { token: ApiToken }) {
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteToken();
  const { push } = useToast();
  const td = "px-3 py-2 text-sm";
  return (
    <tr style={{ borderTop: "1px solid var(--border)" }}>
      <td className={td}>{token.name}</td>
      <td className={td}>{token.last_used_at ?? "never"}</td>
      <td className={td}>{token.created_at}</td>
      <td className={`${td} flex gap-2`}>
        {confirming ? (
          <>
            <button
              type="button"
              disabled={remove.isPending}
              onClick={() =>
                remove.mutate(token.id, {
                  onSuccess: () => {
                    push({ kind: "ok", message: `Revoked ${token.name}` });
                    setConfirming(false);
                  },
                  onError: (err) => {
                    push({ kind: "error", message: err instanceof Error ? err.message : "Revoke failed" });
                    setConfirming(false);
                  },
                })
              }
              style={{ color: "var(--bad)" }}
            >
              Confirm
            </button>
            <button type="button" onClick={() => setConfirming(false)} style={{ color: "var(--muted)" }}>Cancel</button>
          </>
        ) : (
          <button type="button" onClick={() => setConfirming(true)} style={{ color: "var(--bad)" }}>Revoke</button>
        )}
      </td>
    </tr>
  );
}

export function TokensSection() {
  const { data: tokens, isPending, isError } = useTokens();
  const [newSecret, setNewSecret] = useState<string | null>(null);
  const th = "px-3 py-2 text-left text-xs font-medium";

  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-semibold">API tokens</h2>

      {newSecret !== null && <NewSecret secret={newSecret} onDismiss={() => setNewSecret(null)} />}

      {isPending ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>Loading tokens...</p>
      ) : isError ? (
        <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load tokens.</p>
      ) : (tokens ?? []).length === 0 ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>No tokens.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}>Name</th>
              <th className={th} style={{ color: "var(--muted)" }}>Last used</th>
              <th className={th} style={{ color: "var(--muted)" }}>Created</th>
              <th className={th} style={{ color: "var(--muted)" }}></th>
            </tr>
          </thead>
          <tbody>
            {(tokens ?? []).map((t) => (
              <TokenRow key={t.id} token={t} />
            ))}
          </tbody>
          </table>
        </div>
      )}

      <CreateTokenForm onCreated={setNewSecret} />
    </section>
  );
}
