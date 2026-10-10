import { describe, it, expect, vi } from "vitest";
import { getJson, printBatch, renderBatch, sentMessage, ApiError } from "./client";

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

  it("printBatch posts to /api/print and returns the summary", async () => {
    const summary = { total: 2, sent: 2, failed: [], jobs: 2 };
    const fetchMock = vi.fn<typeof fetch>(async () =>
      new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } }));
    vi.stubGlobal("fetch", fetchMock);
    const body = { template: "t", printer: "p", labels: [{ data: {} }, { data: {} }] };
    expect(await printBatch(body)).toEqual(summary);
    expect(fetchMock.mock.calls[0][0]).toBe("/api/print");
    expect(JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string)).toEqual(body);
  });

  it("renderBatch posts to /api/render and returns the attachment", async () => {
    const fetchMock = vi.fn<typeof fetch>(async () =>
      new Response(new Blob(["PK"]), {
        status: 200,
        headers: { "content-type": "application/zip", "content-disposition": 'attachment; filename="t.zip"' },
      }));
    vi.stubGlobal("fetch", fetchMock);
    const body = { template: "t", labels: [{ data: {} }], format: "pdf" as const };
    const result = await renderBatch(body);
    expect(fetchMock.mock.calls[0][0]).toBe("/api/render");
    expect(JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string)).toEqual(body);
    expect(result.filename).toBe("t.zip");
    expect(result.blob).toBeInstanceOf(Blob);
  });

  it("sentMessage names the printer and every failed label", () => {
    expect(sentMessage({ total: 2, sent: 2, failed: [], jobs: 2 }, "Brother")).toBe("Sent 2 labels to Brother");
    const failed = [{ index: 0, error: "refused" }, { index: 2, error: "unreachable" }];
    expect(sentMessage({ total: 3, sent: 1, failed, jobs: 3 }, "Brother"))
      .toBe("Sent 1 of 3 labels to Brother — label 1: refused; label 3: unreachable");
  });
});
