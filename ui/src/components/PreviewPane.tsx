export interface PreviewState {
  url?: string;
  error?: string;
  loading: boolean;
}

export function PreviewPane({
  name,
  format,
  preview,
}: {
  name: string;
  format: "single" | "sheet";
  preview: PreviewState;
}) {
  const isSheet = format === "sheet";
  return (
    <div
      className="flex min-h-48 items-center justify-center rounded-lg border p-4"
      style={{ background: "var(--bg)", borderColor: "var(--border)" }}
    >
      {preview.loading && <p style={{ color: "var(--muted)" }}>rendering preview…</p>}
      {!preview.loading && preview.error && (
        <p style={{ color: "var(--bad)" }}>Preview failed: {preview.error}</p>
      )}
      {!preview.loading && !preview.error && preview.url && !isSheet && (
        <img src={preview.url} alt={`${name} preview`} className="max-h-96 max-w-full" />
      )}
      {!preview.loading && !preview.error && preview.url && isSheet && (
        <object data={preview.url} type="application/pdf" className="h-96 w-full" aria-label={`${name} preview`}>
          <a href={preview.url}>Open sheet preview</a>
        </object>
      )}
      {!preview.loading && !preview.error && !preview.url && (
        <p style={{ color: "var(--muted)" }}>Fill the required fields to preview.</p>
      )}
    </div>
  );
}
