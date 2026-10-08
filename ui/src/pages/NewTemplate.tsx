import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { ApiError } from "../api/client";
import { useCreateTemplate } from "../api/queries";
import { useToast } from "../app/toast-context";

const ID_PATTERN = /^[a-zA-Z0-9_-]+$/;

const PLACEHOLDER = `name: My Label
description: A new label template
unit: mm
dpi: 300
format:
  type: single
  width: 80
  height: 24
layout:
  - type: text
    name: message
    at: [0, 0]
    size: [80, 24]`;

export function NewTemplate() {
  const [id, setId] = useState("");
  const [yaml, setYaml] = useState("");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const navigate = useNavigate();
  const { push } = useToast();
  const save = useCreateTemplate();

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMessage(null);

    const trimmedId = id.trim();
    if (!trimmedId) {
      setErrorMessage("Template ID is required");
      return;
    }
    if (!ID_PATTERN.test(trimmedId)) {
      setErrorMessage("Template ID must be non-empty and match ^[a-zA-Z0-9_-]+$");
      return;
    }

    save.mutate(
      { id: trimmedId, yaml },
      {
        onSuccess: (created) => {
          push({ kind: "ok", message: `Created ${created.id}` });
          navigate(`/templates/${encodeURIComponent(created.id)}`);
        },
        onError: (err) => {
          let message = err instanceof Error ? err.message : "Failed to create template";
          if (err instanceof ApiError && err.status === 409) {
            message = `A template with ID '${trimmedId}' already exists`;
          }
          setErrorMessage(message);
          push({ kind: "error", message });
        },
      },
    );
  };

  return (
    <form onSubmit={onSubmit} className="flex flex-col gap-4 max-w-2xl">
      <h1 className="text-2xl font-semibold">New template</h1>
      <p style={{ color: "var(--muted)" }}>Enter an ID and paste a YAML template definition.</p>

      <div className="flex flex-col gap-1.5">
        <label htmlFor="template-id" className="text-xs font-medium" style={{ color: "var(--muted)" }}>
          Template ID *
        </label>
        <input
          id="template-id"
          type="text"
          value={id}
          onChange={(e) => setId(e.target.value)}
          placeholder="e.g. my-label"
          aria-label="Template ID"
          required
          pattern="[A-Za-z0-9_-]+"
          className="rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
        />
      </div>

      <div className="flex flex-col gap-1.5">
        <label htmlFor="template-yaml" className="text-xs font-medium" style={{ color: "var(--muted)" }}>
          Template YAML (without id) *
        </label>
        <textarea
          id="template-yaml"
          value={yaml}
          onChange={(e) => setYaml(e.target.value)}
          placeholder={PLACEHOLDER}
          spellCheck={false}
          rows={16}
          aria-label="Template YAML"
          className="w-full rounded-md border p-3 font-mono text-sm focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
        />
      </div>

      {errorMessage && <p style={{ color: "var(--bad)" }}>{errorMessage}</p>}

      <div className="flex items-center gap-3">
        <button
          type="submit"
          disabled={save.isPending || id.trim() === "" || yaml.trim() === ""}
          className="rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
        >
          {save.isPending ? "Creating…" : "Create"}
        </button>
      </div>
    </form>
  );
}
