import { Link, Navigate, useParams } from "react-router-dom";
import { useTemplate } from "../api/queries";
import { PrintForm } from "./print/PrintForm";

export function Print() {
  const { templateId } = useParams();
  const selected = templateId ?? "";
  const t = useTemplate(selected);

  // The standalone picker page is gone: /print with no id goes to the grid (ADR-0038).
  if (selected === "") return <Navigate to="/" replace />;

  return (
    <div className="flex flex-col gap-6">
      <Link to="/" className="text-sm underline" style={{ color: "var(--muted)" }}>
        ← All labels
      </Link>
      <h1 className="text-2xl font-semibold">Print</h1>

      {t.isLoading && <p style={{ color: "var(--muted)" }}>loading…</p>}
      {t.isError && (
        <p style={{ color: "var(--bad)" }}>
          {t.error instanceof Error ? t.error.message : "Failed to load template"}
        </p>
      )}
      {t.data && <PrintForm detail={t.data} stale={t.isPlaceholderData} />}
    </div>
  );
}
