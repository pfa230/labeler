import { Link } from "react-router-dom";

/**
 * Shown wherever a page needs a template and none are installed.
 *
 * Nothing is seeded on first run any more (#137), so a new install genuinely has zero templates —
 * without this the Labels grid is a bare sentence and Import/Connect render an empty `<select>` with
 * nowhere to go, which reads as broken rather than as "you haven't installed anything yet".
 */
export function EmptyTemplates({ context }: { context?: string }) {
  return (
    <div
      className="flex flex-col items-start gap-3 rounded-lg border p-6"
      style={{ background: "var(--surface)", borderColor: "var(--border)" }}
    >
      <div>
        <h2 className="font-semibold" style={{ color: "var(--ink)" }}>
          No templates yet
        </h2>
        <p className="text-sm" style={{ color: "var(--muted)" }}>
          {context ?? "Install one from the catalog, or write your own."}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <Link
          to="/templates/catalog"
          className="rounded-md px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
        >
          Browse the catalog
        </Link>
        <Link
          to="/templates/new"
          className="rounded-md border px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2"
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          Paste YAML
        </Link>
        <a
          href="https://github.com/pfa230/labeler/blob/main/docs/SPEC.md#4-layout"
          target="_blank"
          rel="noreferrer"
          className="text-sm underline"
          style={{ color: "var(--muted)" }}
        >
          Template format
        </a>
      </div>
    </div>
  );
}
