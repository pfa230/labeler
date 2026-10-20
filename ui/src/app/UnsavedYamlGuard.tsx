import { useEffect, type ReactNode } from "react";
import { useBeforeUnload, useBlocker } from "react-router-dom";

// A YAML-specific guard: the predicate decides whether the current draft is unsaved, and the guard
// owns the in-app confirmation plus the browser's native unload confirmation. It is deliberately not
// a generic dirty-form registry; only the two YAML editors mount it.
export function UnsavedYamlGuard({
  dirty,
  children,
}: {
  dirty: () => boolean;
  children: ReactNode;
}) {
  const blocker = useBlocker(dirty);

  // A predicate read at event time, not a render-time boolean: a save can update its baseline and
  // navigate in the same tick, and the navigation must not be blocked by the previous render.
  useBeforeUnload((event) => {
    if (!dirty()) return;
    event.preventDefault();
    event.returnValue = "";
  });

  // Drop a pending confirmation if the draft turns clean while it is up, so a stale dialog cannot
  // block a navigation the user has already made possible.
  useEffect(() => {
    if (blocker.state === "blocked" && !dirty()) blocker.reset();
  }, [blocker, dirty]);

  return (
    <>
      {children}
      {blocker.state === "blocked" && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4"
          style={{ background: "rgba(0,0,0,0.4)" }}
        >
          <div
            role="alertdialog"
            aria-modal="true"
            aria-label="Unsaved YAML changes"
            className="flex max-w-sm flex-col gap-3 rounded-lg border p-4"
            style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
          >
            <p>You have unsaved YAML changes. Discard them?</p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => blocker.reset()}
                className="rounded-md px-3 py-2 text-sm"
                style={{ color: "var(--muted)" }}
              >
                Keep editing
              </button>
              <button
                type="button"
                onClick={() => blocker.proceed()}
                className="rounded-md px-3 py-2 text-sm font-medium"
                style={{ color: "var(--bad)" }}
              >
                Discard
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
