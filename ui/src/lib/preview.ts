import { useEffect, useState } from "react";
import { fetchBlob, submitBatch } from "../api/client";
import { defaultOptions, imageFields, referencedFields } from "./templateFields";
import type { TemplateDetail } from "../api/types";

// A 1x1 transparent PNG data URI: a valid sample for data-bound image fields (backend parses a data URI).
const SAMPLE_PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

// Build sample values per referenced field: image fields get a data URI, others the field name as a stand-in.
export function sampleData(fields: string[], imgFields: string[] = []): Record<string, string> {
  const imgs = new Set(imgFields);
  return Object.fromEntries(fields.map((f) => [f, imgs.has(f) ? SAMPLE_PNG : f]));
}

// Renders a preview object URL for a template detail. Single -> /render/label image; sheet -> /batch pdf.
export function useTemplatePreview(detail: TemplateDetail | undefined): { url?: string; error?: string; loading: boolean } {
  // Start in the loading state: TemplateDetail always auto-previews (no fields to fill), so the pane must
  // never flash PreviewPane's "Fill the required fields to preview." idle copy before the effect runs (#74).
  const [state, setState] = useState<{ url?: string; error?: string; loading: boolean }>({ loading: true });
  useEffect(() => {
    if (!detail) return;
    let url: string | undefined;
    let cancelled = false;
    const hasOptions = !!detail.options && Object.keys(detail.options).length > 0;
    const option = hasOptions ? defaultOptions(detail.options) : undefined; // omit `option` for no-option templates
    const sel = option ?? {};
    const data = sampleData(referencedFields(detail.layout, sel), imageFields(detail.layout, sel));
    const label: Record<string, unknown> = option ? { data, option } : { data };
    (async () => {
      setState({ loading: true });
      try {
        let blob: Blob;
        if (detail.format.type === "single") {
          const body = option ? { template: detail.id, data, option } : { template: detail.id, data };
          ({ blob } = await fetchBlob("/render/label", {
            method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body),
          }));
        } else {
          const r = await submitBatch({ template: detail.id, labels: [label], mode: "download" });
          if (r.kind !== "download") throw new Error("expected a sheet PDF");
          blob = r.blob;
        }
        if (cancelled) return;
        url = URL.createObjectURL(blob);
        setState({ url, loading: false });
      } catch (e) {
        if (!cancelled) setState({ error: e instanceof Error ? e.message : "preview failed", loading: false });
      }
    })();
    return () => { cancelled = true; if (url) URL.revokeObjectURL(url); };
  }, [detail]);
  return state;
}
