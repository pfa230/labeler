import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useCreateTemplate } from "../api/queries";
import { useToast } from "../app/toast-context";

const PLACEHOLDER = `id: my-label
name: My Label
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
  const [yaml, setYaml] = useState("");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const navigate = useNavigate();
  const { push } = useToast();
  const create = useCreateTemplate();

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMessage(null);
    create.mutate(yaml, {
      onSuccess: (created) => {
        push({ kind: "ok", message: `Created ${created.id}` });
        navigate(`/templates/${created.id}`);
      },
      onError: (err) => {
        const message = err instanceof Error ? err.message : "Failed to create template";
        setErrorMessage(message);
        push({ kind: "error", message });
      },
    });
  };

  return (
    <form onSubmit={onSubmit} className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">New template</h1>
      <p style={{ color: "var(--muted)" }}>Paste a YAML template definition and create it.</p>

      <textarea
        value={yaml}
        onChange={(e) => setYaml(e.target.value)}
        placeholder={PLACEHOLDER}
        spellCheck={false}
        rows={20}
        aria-label="Template YAML"
        className="w-full rounded-md border p-3 font-mono text-sm focus-visible:outline-none focus-visible:ring-2"
        style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
      />

      {errorMessage && <p style={{ color: "var(--bad)" }}>{errorMessage}</p>}

      <div className="flex items-center gap-3">
        <button
          type="submit"
          disabled={create.isPending || yaml.trim() === ""}
          className="rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
        >
          {create.isPending ? "Creating…" : "Create"}
        </button>
      </div>
    </form>
  );
}
