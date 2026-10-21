import { useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { fetchTemplateSource, useCreateTemplate, useDeleteTemplate, useReplaceTemplate, useTemplate, useTemplateSource } from "../api/queries";
import { ApiError } from "../api/client";
import { cloneTemplateSource } from "../lib/cloneTemplateSource";
import { UnsavedYamlGuard } from "../app/UnsavedYamlGuard";
import { useToast } from "../app/toast-context";
import type { Dimension, TemplateFormat } from "../api/types";
import { FormatBadge } from "../components/FormatBadge";
import { PrintForm } from "./print/PrintForm";

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
      className="rounded-sm border px-1.5 py-0.5 text-xs"
      style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
    >
      {children}
    </code>
  );
}


function RawYamlSection({ id, openForEditing, onDirtyChange }: {
  id: string;
  openForEditing: boolean;
  onDirtyChange: (dirty: boolean) => void;
}) {
  const { data: source, isError: sourceFailed, error: sourceError } = useTemplateSource(id);
  const [draft, setDraft] = useState<string | null>(null);
  const [startEditing, setStartEditing] = useState(openForEditing);
  // The text this edit started from. Not derived from `source`: a failed save drops the source query
  // and the refetch that follows can move `source` to match the draft (the persisted-but-422 case),
  // which would make a modified draft look untouched and skip the discard confirm.
  const [baseline, setBaseline] = useState("");
  const [discarding, setDiscarding] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const save = useReplaceTemplate();
  const { push } = useToast();

  const dirty = draft !== null && draft !== baseline;
  useEffect(() => {
    onDirtyChange(dirty);
    return () => onDirtyChange(false);
  }, [dirty, onDirtyChange]);

  // The source can arrive after mount on a history revisit with an empty query cache.
  if (startEditing && source !== undefined && !sourceFailed) {
    setBaseline(source);
    setDraft(source);
    setStartEditing(false);
  }

  return (
    <UnsavedYamlGuard dirty={() => dirty}>
      <details
        open={openForEditing}
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
              style={{ background: "var(--paper)", color: "var(--ink)" }}
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
              className="mt-3 w-full rounded-md border p-3 font-mono text-sm focus-visible:outline-hidden focus-visible:ring-2"
              style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
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
                style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
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
    </UnsavedYamlGuard>
  );
}

export function TemplateDetail() {
  const { id = "" } = useParams();
  const { data: detail, dataUpdatedAt, isLoading, isError, isPlaceholderData, error } = useTemplate(id);
  const navigate = useNavigate();
  const location = useLocation();
  const [yamlDirty, setYamlDirty] = useState(false);
  const [cloning, setCloning] = useState(false);
  const create = useCreateTemplate();
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteTemplate();
  const { push } = useToast();

  async function clone() {
    setCloning(true);
    try {
      const source = await fetchTemplateSource(id);
      for (let copyNumber = 1; ; copyNumber++) {
        const copyId = `${id}-copy${copyNumber === 1 ? "" : `-${copyNumber}`}`;
        const yaml = cloneTemplateSource(source, copyNumber);
        try {
          await create.mutateAsync({ id: copyId, yaml });
        } catch (err) {
          if (err instanceof ApiError && err.status === 409) continue;
          throw err;
        }
        navigate(`/templates/${copyId}`, { state: { openForEditing: true } });
        break;
      }
    } catch (err) {
      push({ kind: "error", message: err instanceof Error ? err.message : "Clone failed" });
    } finally {
      setCloning(false);
    }
  }

  const variables = detail?.variables ?? [];

  return (
    <div className="flex flex-col gap-6">
      <Link to="/" className="text-sm underline" style={{ color: "var(--muted)" }}>
        ← All labels
      </Link>

      {isLoading && <p style={{ color: "var(--muted)" }}>loading…</p>}
      {!isLoading && (isError || !detail) && (
        <p style={{ color: "var(--bad)" }}>
          {error instanceof Error ? error.message : "Failed to load template"}
        </p>
      )}
      {!isLoading && detail && (
        <>
          <div className="flex flex-wrap items-center justify-between gap-4">
            <div className="flex flex-col gap-1">
              <h1 className="text-2xl font-semibold">{detail.name}</h1>
              <p style={{ color: "var(--muted)" }}>{detail.description}</p>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                disabled={yamlDirty || cloning || isPlaceholderData}
                onClick={clone}
                className="rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50"
                style={{ color: "var(--accent)" }}
              >
                Clone
              </button>
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

          <PrintForm detail={detail} stale={isPlaceholderData} version={dataUpdatedAt} />

          <details
            className="rounded-lg border p-4"
            style={{ background: "var(--surface)", borderColor: "var(--border)" }}
          >
            <summary className="cursor-pointer font-semibold">Details</summary>
            <dl className="mt-3 grid grid-cols-1 gap-x-6 gap-y-2 sm:grid-cols-2">
              <div className="flex justify-between gap-2">
                <dt style={{ color: "var(--muted)" }}>Format</dt>
                <dd>
                  <FormatBadge
                    format={
                      detail.format.type === "sheet"
                        ? { type: "sheet", count: detail.format.positions.length }
                        : { type: "single" }
                    }
                  />
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
          </details>

          {variables.length > 0 && (
            <details
              className="rounded-lg border p-4"
              style={{ background: "var(--surface)", borderColor: "var(--border)" }}
            >
              <summary className="cursor-pointer font-semibold">Variables used</summary>
              <div className="mt-3 flex flex-wrap gap-2">
                {variables.map((s) => (
                  <Chip key={s}>{s}</Chip>
                ))}
              </div>
            </details>
          )}

          {/* Keyed by the route param, not detail.id: React Router reuses this component across
              /templates/a -> /templates/b, and `useTemplate` serves the previous detail as placeholder
              data while the new one loads, so detail.id lags the URL. Keying on it would leave a's draft
              mounted — and savable against a — while the address bar already reads b. The remount is how
              editor state is dropped (an effect that reset it would be setState-in-effect, which the
              lint rule rightly rejects). */}
          <RawYamlSection
            key={id}
            id={id}
            openForEditing={location.state?.openForEditing === true}
            onDirtyChange={setYamlDirty}
          />
        </>
      )}
    </div>
  );
}
