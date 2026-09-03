import { useEffect, useState } from "react";
import { fetchBlob, submitBatch } from "../api/client";
import type { InputSpec, TemplateDetail } from "../api/types";

// A 1x1 transparent PNG data URI: a valid sample for data-bound image fields (backend parses a data URI).
export const SAMPLE_PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

// Build sample values by the thumbnail rule over inputs.all:
// Required interpolated images get SAMPLE_PNG, required interpolated numbers get min ?? 1,
// required checkboxes get false, required dates/datetimes get current instant (RFC 3339 with Z),
// required lists get [input.name], every enum in inputs.all gets its first allowed value, and required strings get the input name.
export function sampleData(inputs: InputSpec[], now: Date = new Date()): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const input of inputs) {
    if (input.control === "select" || (input.values && input.values.length > 0)) {
      if (input.values && input.values.length > 0) {
        data[input.name] = input.values[0];
      }
    } else if (input.interpolated && input.required) {
      if (input.control === "image") {
        data[input.name] = SAMPLE_PNG;
      } else if (input.control === "integer" || input.control === "number") {
        data[input.name] = input.min ?? 1;
      } else if (input.control === "checkbox") {
        data[input.name] = false;
      } else if (input.control === "date" || input.control === "datetime") {
        data[input.name] = now.toISOString();
      } else if (input.control === "list") {
        data[input.name] = [input.name];
      } else {
        data[input.name] = input.name;
      }
    }
  }
  return data;
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
    const data = sampleData(detail.inputs.all);
    const label: Record<string, unknown> = { data };
    (async () => {
      setState({ loading: true });
      try {
        let blob: Blob;
        if (detail.format.type === "single") {
          const body = { template: detail.id, data };
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
