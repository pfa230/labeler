import { useNavigate, useParams } from "react-router-dom";
import { useTemplate, useTemplates } from "../api/queries";
import { PrintForm } from "./print/PrintForm";

export function Print() {
  const navigate = useNavigate();
  const { templateId } = useParams();
  const selected = templateId ?? "";
  const templates = useTemplates();
  const t = useTemplate(selected);

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Print</h1>

      <label className="flex max-w-sm flex-col gap-1">
        <span className="text-sm font-medium">Template</span>
        <select
          aria-label="template"
          value={selected}
          onChange={(e) => {
            const id = e.target.value;
            navigate(id ? `/print/${encodeURIComponent(id)}` : "/print");
          }}
          className="w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
        >
          <option value="">— select a template —</option>
          {(templates.data?.templates ?? []).map((tpl) => (
            <option key={tpl.id} value={tpl.id}>
              {tpl.name}
            </option>
          ))}
        </select>
      </label>

      {selected === "" && <p style={{ color: "var(--muted)" }}>Choose a template to start.</p>}
      {selected !== "" && t.isLoading && <p style={{ color: "var(--muted)" }}>loading…</p>}
      {selected !== "" && t.isError && (
        <p style={{ color: "var(--bad)" }}>
          {t.error instanceof Error ? t.error.message : "Failed to load template"}
        </p>
      )}
      {selected !== "" && t.data && <PrintForm detail={t.data} stale={t.isPlaceholderData} />}
    </div>
  );
}
