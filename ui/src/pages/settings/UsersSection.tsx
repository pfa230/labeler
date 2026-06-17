import { useState } from "react";
import { useUsers, useCreateUser, useDeleteUser, useChangePassword } from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { UserSummary } from "../../api/queries";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function AddUserForm() {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const create = useCreateUser();
  const { push } = useToast();

  const submit = () => {
    if (username.trim() === "") {
      setError("username must not be empty");
      return;
    }
    if (password === "") {
      setError("password must not be empty");
      return;
    }
    setError(null);
    create.mutate(
      { username: username.trim(), password },
      {
        onSuccess: () => {
          push({ kind: "ok", message: `Added ${username.trim()}` });
          setUsername("");
          setPassword("");
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Add failed";
          setError(message);
          push({ kind: "error", message });
        },
      },
    );
  };

  return (
    <div className="flex flex-col gap-2 border-t pt-4" style={{ borderColor: "var(--border)" }}>
      <span className="text-sm font-medium">Add a user</span>
      <div className="flex items-end gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>username</span>
          <input aria-label="new username" value={username} onChange={(e) => setUsername(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>password</span>
          <input aria-label="new user password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <button type="button" onClick={submit} disabled={create.isPending} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
          Add user
        </button>
      </div>
      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
    </div>
  );
}

function UserRow({ user }: { user: UserSummary }) {
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteUser();
  const { push } = useToast();
  const td = "px-3 py-2 text-sm";
  return (
    <tr style={{ borderTop: "1px solid var(--border)" }}>
      <td className={td}>{user.username}</td>
      <td className={`${td} flex gap-2`}>
        {confirming ? (
          <>
            <button
              type="button"
              disabled={remove.isPending}
              onClick={() =>
                remove.mutate(user.id, {
                  onSuccess: () => {
                    push({ kind: "ok", message: `Deleted ${user.username}` });
                    setConfirming(false);
                  },
                  onError: (err) => {
                    push({ kind: "error", message: err instanceof Error ? err.message : "Delete failed" });
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
          <button type="button" onClick={() => setConfirming(true)} style={{ color: "var(--bad)" }}>Delete</button>
        )}
      </td>
    </tr>
  );
}

function ChangePasswordForm() {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [error, setError] = useState<string | null>(null);
  const change = useChangePassword();
  const { push } = useToast();

  const submit = () => {
    if (current === "" || next === "") {
      setError("current and new password must not be empty");
      return;
    }
    setError(null);
    change.mutate(
      { current_password: current, new_password: next },
      {
        onSuccess: () => {
          push({ kind: "ok", message: "Password changed" });
          setCurrent("");
          setNext("");
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Change failed";
          setError(message);
          push({ kind: "error", message });
        },
      },
    );
  };

  return (
    <div className="flex flex-col gap-2 border-t pt-4" style={{ borderColor: "var(--border)" }}>
      <span className="text-sm font-medium">Change my password</span>
      <div className="flex items-end gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>current password</span>
          <input aria-label="current password" type="password" value={current} onChange={(e) => setCurrent(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>new password</span>
          <input aria-label="new password value" type="password" value={next} onChange={(e) => setNext(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <button type="button" onClick={submit} disabled={change.isPending} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
          Change password
        </button>
      </div>
      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
    </div>
  );
}

export function UsersSection() {
  const { data: users, isPending, isError } = useUsers();
  const th = "px-3 py-2 text-left text-xs font-medium";

  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-semibold">Users</h2>

      {isPending ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>Loading users...</p>
      ) : isError ? (
        <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load users.</p>
      ) : (users ?? []).length === 0 ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>No users.</p>
      ) : (
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}>Username</th>
              <th className={th} style={{ color: "var(--muted)" }}></th>
            </tr>
          </thead>
          <tbody>
            {(users ?? []).map((u) => (
              <UserRow key={u.id} user={u} />
            ))}
          </tbody>
        </table>
      )}

      <AddUserForm />
      <ChangePasswordForm />
    </section>
  );
}
