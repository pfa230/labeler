import { useState } from "react";
import { useLocation } from "react-router-dom";
import { useTemplate, useTemplates } from "../api/queries";
import { PrintForm } from "./print/PrintForm";

export function Print() {
  const location = useLocation();
  const [templateId, setTemplateId] = useState<string>(
    () => (location.state as { template?: string } | null)?.template ?? "",
  );
  const templates = useTemplates();
  const t = useTemplate(templateId);

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Print</h1>

      <label className="flex max-w-sm flex-col gap-1">
        <span className="text-sm font-medium">Template</span>
        <select
          aria-label="template"
          value={templateId}
          onChange={(e) => setTemplateId(e.target.value)}
          className="w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
        >
          <option value="">select a template</option>
          {(templates.data?.templates ?? []).map((tpl) => (
            <option key={tpl.id} value={tpl.id}>
              {tpl.name}
            </option>
          ))}
        </select>
      </label>

      {templateId === "" && <p style={{ color: "var(--muted)" }}>Choose a template to start.</p>}
      {templateId !== "" && t.isLoading && <p style={{ color: "var(--muted)" }}>loading…</p>}
      {templateId !== "" && t.isError && (
        <p style={{ color: "var(--bad)" }}>
          {t.error instanceof Error ? t.error.message : "Failed to load template"}
        </p>
      )}
      {templateId !== "" && t.data && <PrintForm detail={t.data} stale={t.isPlaceholderData} />}
    </div>
  );
}
