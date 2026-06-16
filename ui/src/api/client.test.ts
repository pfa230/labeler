import { describe, it, expect, vi } from "vitest";
import { getJson, submitBatch, ApiError } from "./client";

describe("api client", () => {
  it("parses JSON on success", async () => {
    vi.stubGlobal("fetch", vi.fn(async () =>
      new Response(JSON.stringify({ templates: [] }), { status: 200, headers: { "content-type": "application/json" } })));
    expect(await getJson("/templates")).toEqual({ templates: [] });
  });

  it("throws ApiError with the error contract on failure", async () => {
    vi.stubGlobal("fetch", vi.fn(async () =>
      new Response(JSON.stringify({ error: { code: "NotFound", message: "nope" } }),
        { status: 404, headers: { "content-type": "application/json" } })));
    await expect(getJson("/templates/x")).rejects.toMatchObject({ code: "NotFound", status: 404 });
    await expect(getJson("/templates/x")).rejects.toBeInstanceOf(ApiError);
  });

  it("submitBatch returns a summary on a JSON 2xx", async () => {
    const summary = { total: 1, succeeded: 1, failed: [], jobs: 1 };
    vi.stubGlobal("fetch", vi.fn(async () =>
      new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } })));
    expect(await submitBatch({})).toEqual({ kind: "summary", summary });
  });

  it("submitBatch returns a download on a binary 2xx", async () => {
    vi.stubGlobal("fetch", vi.fn(async () =>
      new Response(new Blob(["%PDF"]), {
        status: 200,
        headers: { "content-type": "application/pdf", "content-disposition": 'attachment; filename="x.pdf"' },
      })));
    const result = await submitBatch({});
    expect(result.kind).toBe("download");
    if (result.kind === "download") {
      expect(result.filename).toBe("x.pdf");
      expect(result.blob).toBeInstanceOf(Blob);
    }
  });
});
