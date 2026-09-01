import { useEffect, useRef, useState } from "react";
import type { InputSpec, ParamValue, TemplateInputsRequest, TemplateInputsResponse } from "../api/types";

// A parameter name reserves no words, so `constructor` and `__proto__` are legal ones. Reading
// `o[name]`, testing `name in o` and assigning `o[name] = v` each consult `Object.prototype` for such a
// name: the first two answer for an entry nobody holds, and the third writes no own entry at all. Every
// access keyed by a parameter name goes through these three, never through the operators.
export const hasOwnKey = (o: object, k: string): boolean => Object.prototype.hasOwnProperty.call(o, k);

export function getOwnKey<T>(o: Record<string, T>, k: string): T | undefined {
  return hasOwnKey(o, k) ? o[k] : undefined;
}

export function setOwnKey<T>(o: Record<string, T>, k: string, v: T): void {
  Object.defineProperty(o, k, { value: v, writable: true, enumerable: true, configurable: true });
}

export function seedDefaultValue(input: InputSpec): ParamValue {
  if (
    input.control === "datetime" &&
    typeof input.default === "string" &&
    /^\d{4}-\d{2}-\d{2}$/.test(input.default)
  ) {
    return `${input.default}T00:00`;
  }
  return input.default!;
}

const sortObj = (o?: Record<string, unknown>) =>
  o ? Object.fromEntries(Object.entries(o).sort(([a], [b]) => a.localeCompare(b))) : null;

export function labelInputsKey(templateId: string, data: Record<string, unknown>): string {
  return JSON.stringify([templateId, sortObj(data)]);
}

export async function fetchTemplateInputs(
  templateId: string,
  labels: { data?: Record<string, unknown> }[],
  signal?: AbortSignal,
): Promise<InputSpec[][]> {
  const body: TemplateInputsRequest = { labels };
  const res = await fetch(`/api/templates/${encodeURIComponent(templateId)}/inputs`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    signal,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => null);
    throw new Error(err?.error?.message ?? `Failed to derive inputs (${res.status})`);
  }
  const json = (await res.json()) as TemplateInputsResponse;
  return json.inputs;
}

const CACHE_MAX = 50;

export interface LabelInputsState {
  inputs: InputSpec[];
  pending: boolean;
  error?: string;
}

// Debounced, abortable, LRU-cached label inputs hook.
// Render output is derived strictly from STATE to respect react-hooks/refs.
// The caller derives the request map from the list currently reported, rather than passing a map:
// the list must be requested for the label the caller would actually submit, and what it submits
// depends on which names that same list reports. Deriving it here closes that loop, and a derived
// map that differs from the one the held list answered leaves `pending` true until it is answered.
export function useLabelInputs(
  templateId: string | undefined,
  deriveData: (currentInputs: InputSpec[]) => Record<string, unknown>,
  fallbackInputs: InputSpec[] = [],
  debounceMs = 150,
): LabelInputsState {
  const cache = useRef<Map<string, InputSpec[]>>(new Map());
  const [st, setSt] = useState<{
    templateId?: string;
    key: string;
    inputs: InputSpec[];
    pending: boolean;
    error?: string;
  }>({
    templateId,
    key: "",
    inputs: fallbackInputs,
    pending: !!templateId,
  });

  // A held list describes the template it was requested for and no other, so one belonging to a
  // previous template is dropped rather than reported. `pending` hides it only while a request is in
  // flight; a failed request clears `pending`, and the caller would then read the previous template's
  // entries as this one's and seed them into state that was just reset from the new template.
  const held = st.templateId === templateId ? st.inputs : [];
  const currentInputs = held.length > 0 ? held : fallbackInputs;
  const data = deriveData(currentInputs);
  const key = templateId ? labelInputsKey(templateId, data) : "";

  useEffect(() => {
    if (!templateId) return;

    const cached = cache.current.get(key);
    const controller = new AbortController();
    const timer = setTimeout(async () => {
      if (cached) {
        setSt({ templateId, key, inputs: cached, pending: false });
        return;
      }
      setSt((prev) => ({ ...prev, key, pending: true, error: undefined }));
      try {
        const res = await fetchTemplateInputs(templateId, [{ data }], controller.signal);
        if (controller.signal.aborted) return;
        const derived = res[0] ?? fallbackInputs;
        if (cache.current.size >= CACHE_MAX) {
          const oldest = cache.current.keys().next().value as string | undefined;
          if (oldest) cache.current.delete(oldest);
        }
        cache.current.set(key, derived);
        setSt({ templateId, key, inputs: derived, pending: false });
      } catch (e) {
        if (controller.signal.aborted || (e as Error).name === "AbortError") return;
        const error = e instanceof Error ? e.message : "Failed to derive inputs";
        setSt((prev) => {
          const kept = prev.templateId === templateId ? prev.inputs : [];
          return {
            templateId,
            key,
            inputs: kept.length > 0 ? kept : fallbackInputs,
            pending: false,
            error,
          };
        });
      }
    }, cached ? 0 : debounceMs);

    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [templateId, key, debounceMs]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!templateId) {
    return { inputs: fallbackInputs, pending: false };
  }

  const isCurrent = st.key === key;
  return {
    inputs: currentInputs,
    pending: !isCurrent || st.pending,
    error: isCurrent ? st.error : undefined,
  };
}

export function useBatchRowInputs(
  templateId: string | undefined,
  rows: { id: string; data: Record<string, unknown> }[],
  fallbackInputs: InputSpec[] = [],
): {
  getRowInputs: (rowId: string) => InputSpec[] | undefined;
  rowInputsMap: Map<string, InputSpec[]>;
  pending: boolean;
  error?: string;
} {
  const [cache, setCache] = useState<Map<string, InputSpec[]>>(new Map());
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);

  useEffect(() => {
    if (!templateId || rows.length === 0) return;

    const uncachedRows: { key: string; data: Record<string, unknown> }[] = [];
    const seenKeys = new Set<string>();

    for (const row of rows) {
      const key = labelInputsKey(templateId, row.data);
      if (!cache.has(key) && !seenKeys.has(key)) {
        seenKeys.add(key);
        uncachedRows.push({ key, data: row.data });
      }
    }

    if (uncachedRows.length === 0) return;

    const controller = new AbortController();
    const timer = setTimeout(async () => {
      setPending(true);
      setError(undefined);
      try {
        const res = await fetchTemplateInputs(
          templateId,
          uncachedRows.map((r) => ({ data: r.data })),
          controller.signal,
        );
        if (controller.signal.aborted) return;
        setCache((prev) => {
          const next = new Map(prev);
          res.forEach((specs, idx) => {
            next.set(uncachedRows[idx].key, specs);
          });
          return next;
        });
        setPending(false);
      } catch (e) {
        if (controller.signal.aborted || (e as Error).name === "AbortError") return;
        setError(e instanceof Error ? e.message : "Failed to derive row inputs");
        setPending(false);
      }
    }, 0);

    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [templateId, rows, cache]);

  const rowInputsMap = new Map<string, InputSpec[]>();
  if (templateId) {
    for (const row of rows) {
      const key = labelInputsKey(templateId, row.data);
      const cached = cache.get(key);
      if (cached) {
        rowInputsMap.set(row.id, cached);
      }
    }
  }

  const allResolved = rows.every((row) =>
    templateId ? cache.has(labelInputsKey(templateId, row.data)) : true,
  );

  return {
    getRowInputs: (rowId: string) =>
      rowInputsMap.get(rowId) ?? (fallbackInputs.length > 0 ? fallbackInputs : undefined),
    rowInputsMap,
    pending: pending || !allResolved,
    error,
  };
}

export function pruneDataForSubmit(
  data: Record<string, unknown>,
  activeInputs: InputSpec[],
  deferred?: Record<string, boolean>,
): Record<string, ParamValue> {
  const result: Record<string, ParamValue> = {};
  const activeMap = new Map(activeInputs.map((i) => [i.name, i]));
  for (const [k, v] of Object.entries(data)) {
    if (deferred && getOwnKey(deferred, k)) continue;
    const input = activeMap.get(k);
    if (!input) continue;
    if (v === "" && input.control !== "text" && input.control !== "textarea" && input.control !== "image") continue;
    if (typeof v === "string" || typeof v === "number" || typeof v === "boolean") {
      setOwnKey(result, k, v);
    }
  }
  return result;
}
