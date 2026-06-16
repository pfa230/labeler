import { Link, useParams } from "react-router-dom";
import { useTemplate, useTemplateSource } from "../api/queries";
import { useTemplatePreview } from "../lib/preview";
import { referencedFields, referencedSettings } from "../lib/templateFields";
import type { Dimension, TemplateDetail as TemplateDetailModel, TemplateFormat } from "../api/types";

function dim(d: Dimension): string {
  if (typeof d === "number") return String(d);
  const lo = d.min ?? "?";
  const hi = d.max ?? "?";
  return `${lo}–${hi}`;
}

function formatDimensions(format: TemplateFormat, unit: string): string {
  if (format.type === "single") return `${dim(format.width)} × ${dim(format.height)} ${unit}`;
  return `${format.label_width} × ${format.label_height} ${unit} on ${format.paper_width} × ${format.paper_height} ${unit} sheet`;
}

function Chip({ children }: { children: React.ReactNode }) {
  return (
    <code
      className="rounded px-1.5 py-0.5 text-xs"
      style={{ background: "var(--bg)", color: "var(--ink)" }}
    >
      {children}
    </code>
  );
}

function PreviewPane({ detail }: { detail: TemplateDetailModel }) {
  const { url, error, loading } = useTemplatePreview(detail);
  return (
    <div
      className="flex min-h-48 items-center justify-center rounded-lg border p-4"
      style={{ background: "var(--bg)", borderColor: "var(--border)" }}
    >
      {loading && <p style={{ color: "var(--muted)" }}>rendering preview…</p>}
      {!loading && error && (
        <p style={{ color: "var(--bad)" }}>Preview failed: {error}</p>
      )}
      {!loading && !error && url && detail.format.type === "single" && (
        <img src={url} alt={`${detail.name} preview`} className="max-h-96 max-w-full" />
      )}
      {!loading && !error && url && detail.format.type === "sheet" && (
        <object data={url} type="application/pdf" className="h-96 w-full" aria-label={`${detail.name} preview`}>
          <a href={url}>Open sheet preview</a>
        </object>
      )}
    </div>
  );
}

export function TemplateDetail() {
  const { id = "" } = useParams();
  const { data: detail, isLoading, isError, error } = useTemplate(id);
  const { data: source } = useTemplateSource(id);

  if (isLoading) return <p style={{ color: "var(--muted)" }}>loading…</p>;
  if (isError || !detail) {
    return (
      <p style={{ color: "var(--bad)" }}>
        {error instanceof Error ? error.message : "Failed to load template"}
      </p>
    );
  }

  // Reference view: show the union of fields across all option branches (empty selection = ungated),
  // consistent with referencedSettings which is also ungated.
  const fields = referencedFields(detail.layout, {});
  const settings = referencedSettings(detail.layout);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold">{detail.name}</h1>
          <p style={{ color: "var(--muted)" }}>{detail.description}</p>
        </div>
        <Link
          to="/print"
          state={{ template: detail.id }}
          className="rounded-md px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
        >
          Use to print
        </Link>
      </div>

      <PreviewPane detail={detail} />

      <section className="flex flex-col gap-2">
        <h2 className="text-lg font-semibold">Details</h2>
        <dl className="grid grid-cols-1 gap-x-6 gap-y-2 sm:grid-cols-2">
          <div className="flex justify-between gap-2">
            <dt style={{ color: "var(--muted)" }}>Format</dt>
            <dd>
              <span
                className="rounded-full px-2 py-0.5 text-xs font-medium"
                style={{ background: "var(--accent-soft)", color: "var(--accent)" }}
              >
                {detail.format.type}
              </span>
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt style={{ color: "var(--muted)" }}>Dimensions</dt>
            <dd>{formatDimensions(detail.format, detail.unit)}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt style={{ color: "var(--muted)" }}>Unit</dt>
            <dd>{detail.unit}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt style={{ color: "var(--muted)" }}>DPI</dt>
            <dd>{detail.dpi}</dd>
          </div>
        </dl>
      </section>

      {detail.options && Object.keys(detail.options).length > 0 && (
        <section className="flex flex-col gap-2">
          <h2 className="text-lg font-semibold">Options</h2>
          <ul className="flex flex-col gap-1">
            {Object.entries(detail.options).map(([name, values]) => (
              <li key={name} className="flex flex-wrap items-center gap-2">
                <span style={{ color: "var(--muted)" }}>{name}:</span>
                {values.map((v) => (
                  <Chip key={v}>{v}</Chip>
                ))}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="flex flex-col gap-2">
        <h2 className="text-lg font-semibold">Referenced fields</h2>
        {fields.length > 0 ? (
          <div className="flex flex-wrap gap-2">
            {fields.map((f) => (
              <Chip key={f}>{f}</Chip>
            ))}
          </div>
        ) : (
          <p style={{ color: "var(--muted)" }}>No data fields referenced.</p>
        )}
      </section>

      {settings.length > 0 && (
        <section className="flex flex-col gap-2">
          <h2 className="text-lg font-semibold">Settings used</h2>
          <div className="flex flex-wrap gap-2">
            {settings.map((s) => (
              <Chip key={s}>{s}</Chip>
            ))}
          </div>
        </section>
      )}

      <details
        className="rounded-lg border p-4"
        style={{ background: "var(--surface)", borderColor: "var(--border)" }}
      >
        <summary className="cursor-pointer font-semibold">Raw YAML</summary>
        <pre
          className="mt-3 overflow-auto rounded-md p-3 text-xs"
          style={{ background: "var(--bg)", color: "var(--ink)" }}
        >
          {source ?? "loading…"}
        </pre>
      </details>
    </div>
  );
}
