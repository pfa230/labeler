import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { ToastContext, type Toast, type ToastKind } from "./toast-context";

const DEDUPE_WINDOW_MS = 4000;
const DISMISS_AFTER_MS = 5000;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(0);
  const recent = useRef(new Map<string, number>());
  const timers = useRef(new Map<number, ReturnType<typeof setTimeout>>());

  const dismiss = useCallback((id: number) => {
    const timer = timers.current.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.current.delete(id);
    }
    setToasts((current) => current.filter((t) => t.id !== id));
  }, []);

  const push = useCallback(
    ({ kind, message }: { kind: ToastKind; message: string }) => {
      const key = `${kind}:${message}`;
      const now = Date.now();

      for (const [k, timestamp] of recent.current) {
        if (now - timestamp >= DEDUPE_WINDOW_MS) {
          recent.current.delete(k);
        }
      }

      const last = recent.current.get(key);
      if (last !== undefined && now - last < DEDUPE_WINDOW_MS) return;
      recent.current.set(key, now);

      const id = nextId.current++;
      setToasts((current) => [...current, { id, kind, message }]);
      const timer = setTimeout(() => {
        timers.current.delete(id);
        dismiss(id);
      }, DISMISS_AFTER_MS);
      timers.current.set(id, timer);
    },
    [dismiss],
  );

  useEffect(() => {
    const activeTimers = timers.current;
    return () => {
      for (const timer of activeTimers.values()) {
        clearTimeout(timer);
      }
      activeTimers.clear();
    };
  }, []);

  return (
    <ToastContext.Provider value={{ push }}>
      {children}
      <ToastRegion toasts={toasts} onDismiss={dismiss} />
    </ToastContext.Provider>
  );
}

function ToastRegion({
  toasts,
  onDismiss,
}: {
  toasts: Toast[];
  onDismiss: (id: number) => void;
}) {
  return (
    <div
      role="status"
      aria-live="polite"
      className="pointer-events-none fixed bottom-4 right-4 z-50 flex flex-col gap-2"
    >
      {toasts.map((toast) => (
        <button
          key={toast.id}
          type="button"
          onClick={() => onDismiss(toast.id)}
          className="pointer-events-auto max-w-sm rounded-md border px-4 py-2 text-left text-sm shadow-lg"
          style={{
            background: "var(--surface)",
            color: "var(--ink)",
            borderColor:
              toast.kind === "error" ? "var(--bad)" : "var(--good)",
          }}
        >
          {toast.message}
        </button>
      ))}
    </div>
  );
}
