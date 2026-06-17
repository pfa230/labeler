import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useLogin } from "../api/auth";

const inputClass =
  "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = {
  background: "var(--surface)",
  borderColor: "var(--border)",
  color: "var(--ink)",
} as const;
const buttonBase =
  "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

export function Login() {
  const navigate = useNavigate();
  const login = useLogin();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    login.mutate(
      { username, password },
      { onSuccess: () => navigate("/") },
    );
  };

  const error = login.error instanceof Error ? login.error.message : null;

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <form onSubmit={onSubmit} className="flex w-full max-w-sm flex-col gap-4">
        <h1 className="text-lg font-semibold">Sign in</h1>
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Username</span>
          <input
            type="text"
            aria-label="username"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            className={inputClass}
            style={inputStyle}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Password</span>
          <input
            type="password"
            aria-label="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className={inputClass}
            style={inputStyle}
          />
        </label>
        {error && <p style={{ color: "var(--bad)" }}>{error}</p>}
        <button
          type="submit"
          disabled={login.isPending}
          className={buttonBase}
          style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
        >
          Sign in
        </button>
      </form>
    </div>
  );
}
