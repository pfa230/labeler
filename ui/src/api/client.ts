import type { ApiErrorBody } from "./types";

const BASE = "/api";

export class ApiError extends Error {
  code: string;
  status: number;
  details?: unknown;
  constructor(status: number, code: string, message: string, details?: unknown) {
    super(message);
    this.status = status; this.code = code; this.details = details;
  }
}

function on401(status: number) {
  if (status === 401) window.dispatchEvent(new CustomEvent("labeler:unauthenticated"));
}

async function toError(res: Response): Promise<ApiError> {
  const ct = res.headers.get("content-type") ?? "";
  if (ct.includes("application/json")) {
    const body = (await res.json()) as ApiErrorBody;
    return new ApiError(res.status, body.error?.code ?? "Unknown", body.error?.message ?? res.statusText, body.error?.details);
  }
  return new ApiError(res.status, "Unknown", await res.text().catch(() => res.statusText));
}

export async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) {
    on401(res.status);
    throw await toError(res);
  }
  return (await res.json()) as T;
}

export async function sendJson<T>(method: string, path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method, headers: { "content-type": "application/json" }, body: JSON.stringify(body),
  });
  if (!res.ok) {
    on401(res.status);
    throw await toError(res);
  }
  return (await res.json()) as T;
}

// DELETE returns 204 with no body, so there is nothing to parse; throw the error contract on non-2xx.
export async function del(path: string): Promise<void> {
  const res = await fetch(`${BASE}${path}`, { method: "DELETE" });
  if (!res.ok) throw await toError(res);
}

function filenameFrom(res: Response): string | undefined {
  // Matches the current server's `Content-Disposition: attachment; filename="x"`; not RFC5987 `filename*=`.
  const m = (res.headers.get("content-disposition") ?? "").match(/filename="?([^"]+)"?/);
  return m?.[1];
}

// /api/render/label: 2xx is ALWAYS a binary image/pdf; failure is the JSON error contract.
export async function fetchBlob(path: string, init?: RequestInit): Promise<{ blob: Blob; filename?: string }> {
  const res = await fetch(`${BASE}${path}`, init);
  if (!res.ok) {
    on401(res.status);
    throw await toError(res);
  }
  return { blob: await res.blob(), filename: filenameFrom(res) };
}

// /api/batch: a 2xx is EITHER a binary download (zip/pdf) OR a JSON print summary, depending on `mode`.
// Discriminate on content-type after confirming res.ok; errors are still the JSON contract.
import type { BatchSummary } from "./types";
export type BatchResult =
  | { kind: "download"; blob: Blob; filename?: string }
  | { kind: "summary"; summary: BatchSummary };

export async function printLabel(body: {
  template: string;
  printer: string; // /print's PrintRequest.printer is required (no serde default)
  fields: Record<string, string>;
  option?: Record<string, string>;
  copies: number;
}): Promise<BatchSummary> {
  return sendJson<BatchSummary>("POST", "/print", body);
}

export async function submitBatch(body: unknown): Promise<BatchResult> {
  const res = await fetch(`${BASE}/batch`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body),
  });
  if (!res.ok) {
    on401(res.status);
    throw await toError(res);
  }
  const ct = res.headers.get("content-type") ?? "";
  if (ct.includes("application/json")) {
    return { kind: "summary", summary: (await res.json()) as BatchSummary };
  }
  return { kind: "download", blob: await res.blob(), filename: filenameFrom(res) };
}

// Trigger a browser download. Revoke the object URL on a delay, immediate revoke after click()
// can abort the download in Chromium (crbug 41380177); MDN: revoke when finished using the URL.
export function saveBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = filename; document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}
