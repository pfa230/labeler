import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { browseConnection, materializeConnection } from "./connectors";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

describe("connectors api", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => vi.unstubAllGlobals());

  it("browseConnection posts the request and returns the page", async () => {
    const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
      json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }], next_cursor: null, has_more: false, count: 1 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const page = await browseConnection("c1", { resource: "entities" });
    expect(page.rows[0].id.key).toBe("e1");
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe("/api/connections/c1/browse");
    expect((init as RequestInit).method).toBe("POST");
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ resource: "entities" });
  });

  it("materializeConnection returns label rows", async () => {
    const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () => json([{ source: { resource: "entities", key: "e1" }, data: { name: "Drill" } }]));
    vi.stubGlobal("fetch", fetchMock);
    const rows = await materializeConnection("c1", { rows: [{ resource: "entities", key: "e1" }], fields: ["name"], expansion: "as_listed" });
    expect(rows[0].data.name).toBe("Drill");
  });
});
