import { useEffect } from "react";
import { useTemplates } from "../api/queries";
import { useToast } from "../app/toast";

export function Templates() {
  const { data, isLoading, isError, error } = useTemplates();
  const { push } = useToast();

  useEffect(() => {
    if (isError) {
      push({
        kind: "error",
        message: error instanceof Error ? error.message : "Failed to load templates",
      });
    }
  }, [isError, error, push]);

  return (
    <div>
      <h1 className="text-2xl font-semibold">Templates</h1>
      {isLoading && <p style={{ color: "var(--muted)" }}>loading…</p>}
      {isError && (
        <p style={{ color: "var(--bad)" }}>
          {error instanceof Error ? error.message : "Failed to load templates"}
        </p>
      )}
      {data && (
        <p style={{ color: "var(--muted)" }}>
          {data.templates.length} template{data.templates.length === 1 ? "" : "s"} available
        </p>
      )}
    </div>
  );
}
