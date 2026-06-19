import { useEffect, useRef, useState } from "react";

export interface PreviewInput {
  templateId: string;
  format: "single" | "sheet";   // the template's format type
  data: Record<string, string>;
  option?: Record<string, string>;
  startSlot?: number;
}

function hasOpt(o?: Record<string, string>): o is Record<string, string> {
  return !!o && Object.keys(o).length > 0;
}
const sortObj = (o?: Record<string, string>) =>
  o ? Object.fromEntries(Object.entries(o).sort(([a], [b]) => a.localeCompare(b))) : null;

export function previewKey(i: PreviewInput): string {
  return JSON.stringify([i.templateId, i.format, sortObj(i.data), hasOpt(i.option) ? sortObj(i.option) : null, i.startSlot ?? 0]);
}

interface PreviewState { url?: string; error?: string; loading: boolean }
const CACHE_MAX = 12;

// Debounced, abortable, capped-cache live preview. `enabled` gates rendering (required fields present).
// Render output is derived from STATE + the `enabled` PARAM only; the ref-backed cache is read solely
// inside the effect (the repo's `react-hooks/refs` forbids reading refs during render), and every
// `setState` happens inside the async timer (so `react-hooks/set-state-in-effect` does not fire).
export function useLivePreview(input: PreviewInput, enabled: boolean, debounceMs = 300): PreviewState {
  const key = previewKey(input);
  const cache = useRef<Map<string, string>>(new Map()); // key -> object URL (FIFO-capped)
  const [st, setSt] = useState<{ key: string; url?: string; error?: string; loading: boolean }>({ key: "", loading: false });

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    const cached = cache.current.get(key); // ref read in the EFFECT (allowed), not during render
    const timer = setTimeout(async () => {
      if (cached) { setSt({ key, url: cached, loading: false }); return; }
      setSt({ key, loading: true });
      try {
        const single = input.format === "single";
        const path = single ? "/api/render/label" : "/api/batch";
        const label = { data: input.data, ...(hasOpt(input.option) ? { option: input.option } : {}) };
        const body = single
          ? { template: input.templateId, data: input.data, ...(hasOpt(input.option) ? { option: input.option } : {}) }
          : { template: input.templateId, mode: "download", labels: [label],
              ...(input.startSlot ? { start_slot: input.startSlot } : {}) };
        const res = await fetch(path, {
          method: "POST", headers: { "content-type": "application/json" },
          body: JSON.stringify(body), signal: controller.signal,
        });
        if (!res.ok) {
          const err = await res.json().catch(() => null);
          throw new Error(err?.error?.message ?? `preview failed (${res.status})`);
        }
        const blob = await res.blob();
        if (controller.signal.aborted) return; // unmounted/key-changed during await: drop
        const url = URL.createObjectURL(blob);
        if (cache.current.size >= CACHE_MAX) {
          const oldest = cache.current.keys().next().value as string | undefined;
          if (oldest) { URL.revokeObjectURL(cache.current.get(oldest)!); cache.current.delete(oldest); }
        }
        cache.current.set(key, url);
        setSt({ key, url, loading: false });
      } catch (e) {
        if (controller.signal.aborted || (e as Error).name === "AbortError") return; // stale: drop
        setSt({ key, error: e instanceof Error ? e.message : "preview failed", loading: false });
      }
    }, cached ? 0 : debounceMs);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [key, enabled]); // eslint-disable-line react-hooks/exhaustive-deps -- input captured via `key`; debounceMs treated as constant

  useEffect(() => { const m = cache.current; return () => { for (const u of m.values()) URL.revokeObjectURL(u); }; }, []);

  // Render from state + the enabled param only (NO ref access here):
  if (!enabled) return { loading: false };
  if (st.key === key) return { url: st.url, error: st.error, loading: st.loading };
  return { loading: true }; // a newer key: the effect is debouncing/in flight
}
