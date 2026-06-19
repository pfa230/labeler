import { useEffect, useRef, useState } from "react";
import type { ResolvedLabel } from "./labelGrid";
import type { PreviewState } from "../components/PreviewPane";

export interface RowPreviewInput {
  templateId: string;
  format: "single" | "sheet";
  label?: ResolvedLabel;
  startSlot?: number;
}

export function useRowPreview(input: RowPreviewInput): PreviewState {
  const key = JSON.stringify([input.templateId, input.format, input.label ?? null, input.startSlot ?? 0]);
  const [state, setState] = useState<PreviewState>({ loading: false });
  const urlRef = useRef<string | undefined>(undefined);

  useEffect(() => {
    if (!input.label) return;
    const controller = new AbortController();
    let cancelled = false;
    (async () => {
      setState({ loading: true });
      try {
        const single = input.format === "single";
        const path = single ? "/api/render/label" : "/api/batch";
        const body = single
          ? { template: input.templateId, ...input.label }
          : {
              template: input.templateId,
              mode: "download",
              labels: [input.label],
              ...(input.startSlot ? { start_slot: input.startSlot } : {}),
            };
        const res = await fetch(path, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(body),
          signal: controller.signal,
        });
        if (!res.ok) {
          const err = await res.json().catch(() => null);
          throw new Error(err?.error?.message ?? `preview failed (${res.status})`);
        }
        const blob = await res.blob();
        if (cancelled) return;
        if (urlRef.current) URL.revokeObjectURL(urlRef.current);
        const url = URL.createObjectURL(blob);
        urlRef.current = url;
        setState({ url, loading: false });
      } catch (e) {
        if (!cancelled) setState({ error: e instanceof Error ? e.message : "preview failed", loading: false });
      }
    })();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps -- input captured via `key`

  useEffect(() => () => { if (urlRef.current) URL.revokeObjectURL(urlRef.current); }, []);

  if (!input.label) return { loading: false };
  return state;
}
