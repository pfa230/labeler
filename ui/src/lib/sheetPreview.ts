import { useEffect, useRef, useState } from "react";
import type { ParamValue } from "../api/types";
import type { ResolvedLabel } from "./labelGrid";
import type { PreviewState } from "../components/PreviewPane";

export interface SheetPreviewInput {
  templateId: string;
  labels: ResolvedLabel[];
  startSlot?: number;
}

const sortObj = (o?: Record<string, ParamValue>) =>
  o ? Object.fromEntries(Object.entries(o).sort(([a], [b]) => a.localeCompare(b))) : null;

export function sheetPreviewKey(i: SheetPreviewInput): string {
  const sortedLabels = i.labels.map((l) => ({
    data: sortObj(l.data),
  }));
  return JSON.stringify([i.templateId, sortedLabels, i.startSlot]);
}

export function useSheetPreview(
  input: SheetPreviewInput,
  enabled: boolean,
  debounceMs = 300,
): PreviewState {
  const key = sheetPreviewKey(input);
  const urlRef = useRef<string | undefined>(undefined);
  const [st, setSt] = useState<{ key: string; url?: string; error?: string; loading: boolean }>({
    key: "",
    loading: false,
  });

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    const timer = setTimeout(async () => {
      setSt({ key, loading: true });
      try {
        const body = {
          template: input.templateId,
          mode: "download",
          labels: input.labels,
          ...(input.startSlot ? { start_slot: input.startSlot } : {}),
        };
        const res = await fetch("/api/batch", {
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
        if (controller.signal.aborted) return;
        if (urlRef.current) {
          URL.revokeObjectURL(urlRef.current);
        }
        const url = URL.createObjectURL(blob);
        urlRef.current = url;
        setSt({ key, url, loading: false });
      } catch (e) {
        if (controller.signal.aborted || (e as Error).name === "AbortError") return;
        setSt({ key, error: e instanceof Error ? e.message : "preview failed", loading: false });
      }
    }, debounceMs);

    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [key, enabled, debounceMs]); // eslint-disable-line react-hooks/exhaustive-deps -- input captured via key

  useEffect(() => {
    return () => {
      if (urlRef.current) {
        URL.revokeObjectURL(urlRef.current);
      }
    };
  }, []);

  if (!enabled) return { loading: false };
  if (st.key === key) return { url: st.url, error: st.error, loading: st.loading };
  return { loading: true };
}
