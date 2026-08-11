import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useDeleteTemplate, useReplaceTemplate, useTemplate, useTemplateSource } from "../api/queries";
import { useToast } from "../app/toast-context";
import { useTemplatePreview } from "../lib/preview";
import { referencedFields, referencedVariables } from "../lib/templateFields";
import type { Dimension, TemplateFormat } from "../api/types";
import { PreviewPane } from "../components/PreviewPane";

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


function RawYamlSection({ id }: { id: string }) {
  const { data: source, isError: sourceFailed, error: sourceError } = useTemplateSource(id);
  const [draft, setDraft] = useState<string | null>(null);
  // The text this edit started from. Not derived from `source`: a failed save drops the source query
  // and the refetch that follows can move `source` to match the draft (the persisted-but-422 case),
  // which would make a modified draft look untouched and skip the discard confirm.
  const [baseline, setBaseline] = useState("");
  const [discarding, setDiscarding] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const save = useReplaceTemplate();
  const { push } = useToast();

  return (
      <details
        className="rounded-lg border p-4"
        style={{ background: "var(--surface)", borderColor: "var(--border)" }}
      >
        {/* Every control sits in the body, never in the <summary>: a click anywhere in a summary
            toggles the disclosure, so an Edit button there would open edit mode and collapse the
            panel holding the textarea in the same click (#141). */}
        <summary className="cursor-pointer font-semibold">Raw YAML</summary>
        {draft === null ? (
          <>
            <pre
              className="mt-3 overflow-auto rounded-md p-3 text-xs"
              style={{ background: "var(--bg)", color: "var(--ink)" }}
            >
              {sourceFailed
                ? `Could not load the template source: ${sourceError instanceof Error ? sourceError.message : "unknown error"}`
                : (source ?? "loading…")}
            </pre>
            <button
              type="button"
              // A failed refetch can leave stale data alongside isError, so both checks matter:
              // editing text the server has disowned would save it straight back.
              disabled={sourceFailed || source === undefined}
              onClick={() => {
                setSaveError(null);
                // Reset the confirm too, or edit mode reopens straight into Discard/Keep editing.
                setDiscarding(false);
                setBaseline(source ?? "");
                setDraft(source ?? "");
              }}
              className="mt-3 rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50"
              style={{ color: "var(--accent)" }}
            >
              Edit
            </button>
          </>
        ) : (
          <>
            <textarea
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              spellCheck={false}
              rows={20}
              aria-label="Template YAML"
              className="mt-3 w-full rounded-md border p-3 font-mono text-sm focus-visible:outline-none focus-visible:ring-2"
              style={{ background: "var(--bg)", borderColor: "var(--border)", color: "var(--ink)" }}
            />
            {saveError && <p style={{ color: "var(--bad)" }}>{saveError}</p>}
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <button
                type="button"
                disabled={save.isPending}
                onClick={() =>
                  save.mutate(
                    { id: id, yaml: draft },
                    {
                      onSuccess: () => {
                        setDraft(null);
                        setDiscarding(false);
                        setSaveError(null);
                        push({ kind: "ok", message: `Saved ${id}` });
                      },
                      onError: (err) => {
                        // Inline as well as the toast: a validation error names a path
                        // (layout[2].size) that belongs next to the text it refers to.
                        const message = err instanceof Error ? err.message : "Save failed";
                        setSaveError(message);
                        push({ kind: "error", message });
                      },
                    },
                  )
                }
                className="rounded-md px-3 py-2 text-sm font-medium"
                style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
              >
                Save
              </button>
              {discarding ? (
                <>
                  <span style={{ color: "var(--muted)" }}>Discard changes?</span>
                  <button
                    type="button"
                    onClick={() => {
                      setDiscarding(false);
                      setDraft(null);
                      setSaveError(null);
                    }}
                    className="rounded-md px-3 py-2 text-sm"
                    style={{ color: "var(--bad)" }}
                  >
                    Discard
                  </button>
                  <button
                    type="button"
                    onClick={() => setDiscarding(false)}
                    className="rounded-md px-3 py-2 text-sm"
                    style={{ color: "var(--muted)" }}
                  >
                    Keep editing
                  </button>
                </>
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    // Ask only when there is something to lose, measured against the text this edit
                    // started from rather than whatever the source query holds now.
                    if (draft !== baseline) setDiscarding(true);
                    else setDraft(null);
                  }}
                  className="rounded-md px-3 py-2 text-sm"
                  style={{ color: "var(--muted)" }}
                >
                  Cancel
                </button>
              )}
            </div>
          </>
        )}
      </details>
  );
}

export function TemplateDetail() {
  const { id = "" } = useParams();
  const { data: detail, isLoading, isError, error } = useTemplate(id);
  const { url: previewUrl, error: previewError, loading: previewLoading } = useTemplatePreview(detail);
  const navigate = useNavigate();
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteTemplate();
  const { push } = useToast();

  if (isLoading) return <p style={{ color: "var(--muted)" }}>loading…</p>;
  if (isError || !detail) {
    return (
      <p style={{ color: "var(--bad)" }}>
        {error instanceof Error ? error.message : "Failed to load template"}
      </p>
    );
  }

  // Reference view: show the union of fields across all option branches (empty selection = ungated),
  // consistent with referencedVariables which is also ungated.
  const fields = referencedFields(detail.layout, {});
  const variables = referencedVariables(detail.layout);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold">{detail.name}</h1>
          <p style={{ color: "var(--muted)" }}>{detail.description}</p>
        </div>
        <div className="flex items-center gap-2">
          <Link
            to={`/print/${encodeURIComponent(detail.id)}`}
            className="rounded-md px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2"
            style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
          >
            Use to print
          </Link>
          {confirming ? (
            <>
              <button
                type="button"
                disabled={remove.isPending}
                onClick={() =>
                  remove.mutate(detail.id, {
                    onSuccess: () => {
                      push({ kind: "ok", message: `Deleted ${detail.id}` });
                      // The list is the index route; "/templates" is only a redirect to it.
                      navigate("/");
                    },
                    onError: (err) => {
                      // Restore the plain Delete button: leaving Confirm/Cancel up reads as a retry
                      // prompt for something that already ran.
                      setConfirming(false);
                      push({
                        kind: "error",
                        message: err instanceof Error ? err.message : "Delete failed",
                      });
                    },
                  })
                }
                className="rounded-md px-3 py-2 text-sm font-medium"
                style={{ color: "var(--bad)" }}
              >
                Confirm
              </button>
              <button
                type="button"
                onClick={() => setConfirming(false)}
                className="rounded-md px-3 py-2 text-sm"
                style={{ color: "var(--muted)" }}
              >
                Cancel
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={() => setConfirming(true)}
              className="rounded-md px-3 py-2 text-sm font-medium"
              style={{ color: "var(--bad)" }}
            >
              Delete
            </button>
          )}
        </div>
      </div>

      <PreviewPane name={detail.name} format={detail.format.type} preview={{ url: previewUrl, error: previewError, loading: previewLoading }} />

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

      {variables.length > 0 && (
        <section className="flex flex-col gap-2">
          <h2 className="text-lg font-semibold">Variables used</h2>
          <div className="flex flex-wrap gap-2">
            {variables.map((s) => (
              <Chip key={s}>{s}</Chip>
            ))}
          </div>
        </section>
      )}

      {/* Keyed by id: React Router reuses this component across /templates/a -> /templates/b, and a
          remount is how editor state is dropped on that move (an effect that reset it would be
          setState-in-effect, which the lint rule rightly rejects). */}
      <RawYamlSection key={detail.id} id={detail.id} />
    </div>
  );
}
