import React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  browseConnection,
  materializeConnection,
  useConnectorSchema,
  useSaveConnection,
  useDeleteConnection,
  useSetDefaultConnection,
  useClearDefaultConnection,
} from "./connectors";

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

  it("useSaveConnection removes queries appropriately for update and create, and carries mutationKey", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const removeSpy = vi.spyOn(qc, "removeQueries");

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method || "GET").toUpperCase();
      if (url.includes("/api/connections/c1") && method === "PUT") {
        return json({ id: "c1", name: "Updated Conn", connector: "mock", base_url: "http://example.com", enabled: true, has_credential: false, transforms: [] });
      }
      if (url === "/api/connections" && method === "POST") {
        return json({ id: "c2", name: "New Conn", connector: "mock", base_url: "http://example.com", enabled: true, has_credential: false, transforms: [] });
      }
      return json({}, 404);
    });
    vi.stubGlobal("fetch", fetchMock);

    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);

    const { result } = renderHook(() => useSaveConnection(), { wrapper });

    // 1. Update
    await act(async () => {
      await result.current.mutateAsync({
        id: "c1",
        input: { name: "Updated Conn", connector: "mock", base_url: "http://example.com" },
      });
    });

    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["connections"] });
    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["connector-schema", "c1"] });

    removeSpy.mockClear();

    // 2. Create
    await act(async () => {
      await result.current.mutateAsync({
        input: { name: "New Conn", connector: "mock", base_url: "http://example.com" },
      });
    });

    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["connections"] });
    expect(removeSpy).not.toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["connector-schema", expect.anything()] }));
    expect(qc.getMutationCache().getAll()[0].options.mutationKey).toEqual(["connection"]);
  });

  it("useDeleteConnection removes connections, connector-schema and settings queries and carries mutationKey", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const removeSpy = vi.spyOn(qc, "removeQueries");

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method || "GET").toUpperCase();
      if (url.includes("/api/connections/c1") && method === "DELETE") {
        return json({ ok: true });
      }
      return json({}, 404);
    });
    vi.stubGlobal("fetch", fetchMock);

    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);

    const { result } = renderHook(() => useDeleteConnection(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync("c1");
    });

    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["connections"] });
    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["connector-schema", "c1"] });
    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["settings"] });
    expect(qc.getMutationCache().getAll()[0].options.mutationKey).toEqual(["connection"]);
  });

  it("useSaveConnection and useDeleteConnection do not dispatch window events", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });

    const events: Array<{ type: string; detail: unknown }> = [];
    const saveHandler = (e: Event) => events.push({ type: e.type, detail: (e as CustomEvent).detail });
    const deleteHandler = (e: Event) => events.push({ type: e.type, detail: (e as CustomEvent).detail });

    window.addEventListener("labeler:connection-saved", saveHandler);
    window.addEventListener("labeler:connection-deleted", deleteHandler);

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method || "GET").toUpperCase();
      if (url.includes("/api/connections/c1") && method === "PUT") {
        return json({ id: "c1", name: "Saved", connector: "mock", base_url: "http://example.com", enabled: true, has_credential: false, transforms: [] });
      }
      if (url.includes("/api/connections/c1") && method === "DELETE") {
        return json({ ok: true });
      }
      return json({}, 404);
    });
    vi.stubGlobal("fetch", fetchMock);

    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);

    const { result: saveResult } = renderHook(() => useSaveConnection(), { wrapper });
    const { result: deleteResult } = renderHook(() => useDeleteConnection(), { wrapper });

    await act(async () => {
      await saveResult.current.mutateAsync({
        id: "c1",
        input: { name: "Saved", connector: "mock", base_url: "http://example.com" },
      });
    });
    await act(async () => {
      await deleteResult.current.mutateAsync("c1");
    });

    window.removeEventListener("labeler:connection-saved", saveHandler);
    window.removeEventListener("labeler:connection-deleted", deleteHandler);

    expect(events).toEqual([]);
  });

  it("a save while the first schema read is still in flight starts a fresh read and ignores the abandoned response", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });

    let resolveSchema1!: (resp: Response) => void;
    const schema1Promise = new Promise<Response>((res) => {
      resolveSchema1 = res;
    });

    let schemaCallCount = 0;
    const oldSchema = {
      version: "1.0",
      resources: [{ id: "entities", label: "Entities Old", view: "table", columns: [], filters: [], dynamic_source_prefix: null, fields_incomplete: false }],
      relationships: [],
    };
    const newSchema = {
      version: "1.0",
      resources: [{ id: "entities", label: "Entities New", view: "table", columns: [], filters: [], dynamic_source_prefix: null, fields_incomplete: false }],
      relationships: [],
    };

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method || "GET").toUpperCase();
      if (url.includes("/api/connections/c1/schema") && method === "GET") {
        schemaCallCount++;
        if (schemaCallCount === 1) {
          return schema1Promise;
        }
        return json(newSchema);
      }
      if (url.includes("/api/connections/c1") && method === "PUT") {
        return json({ id: "c1", name: "Saved", connector: "mock", base_url: "http://example.com", enabled: true, has_credential: false, transforms: [] });
      }
      return json({}, 404);
    });
    vi.stubGlobal("fetch", fetchMock);

    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);

    const { result: schemaResult } = renderHook(() => useConnectorSchema("c1"), { wrapper });
    const { result: saveResult } = renderHook(() => useSaveConnection(), { wrapper });

    // Wait for the first schema request to be issued and pending
    await waitFor(() => expect(schemaCallCount).toBe(1));
    expect(schemaResult.current.isLoading).toBe(true);

    // Now save the connection while schema request 1 is still in flight
    await act(async () => {
      await saveResult.current.mutateAsync({
        id: "c1",
        input: { name: "Saved", connector: "mock", base_url: "http://example.com" },
      });
    });

    // A fresh read (e.g. on next visit or refetch) is issued
    await act(async () => {
      schemaResult.current.refetch();
    });

    // Fresh read resolves with newSchema
    await waitFor(() => expect(schemaCallCount).toBe(2));
    await waitFor(() => expect(schemaResult.current.data?.resources[0].label).toBe("Entities New"));

    // Now resolve the abandoned first schema request afterwards
    await act(async () => {
      resolveSchema1(json(oldSchema));
    });

    // It must NOT replace the new schema
    expect(schemaResult.current.data?.resources[0].label).toBe("Entities New");
  });

  it("failed mutations evict nothing", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const removeSpy = vi.spyOn(qc, "removeQueries");

    const fetchMock = vi.fn(async () => json({ error: "failed" }, 500));
    vi.stubGlobal("fetch", fetchMock);

    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);

    const { result: saveResult } = renderHook(() => useSaveConnection(), { wrapper });
    const { result: deleteResult } = renderHook(() => useDeleteConnection(), { wrapper });

    await act(async () => {
      await expect(
        saveResult.current.mutateAsync({
          id: "c1",
          input: { name: "Saved", connector: "mock", base_url: "http://example.com" },
        }),
      ).rejects.toThrow();
    });

    await act(async () => {
      await expect(deleteResult.current.mutateAsync("c1")).rejects.toThrow();
    });

    expect(removeSpy).not.toHaveBeenCalled();
  });

  it("useSetDefaultConnection sends PUT /settings/default_connection_id, removes settings and carries mutationKey", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const removeSpy = vi.spyOn(qc, "removeQueries");

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method || "GET").toUpperCase();
      if (url.includes("/api/settings/default_connection_id") && method === "PUT") {
        return json({ value: "c1", is_default: false });
      }
      return json({}, 404);
    });
    vi.stubGlobal("fetch", fetchMock);

    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);

    const { result } = renderHook(() => useSetDefaultConnection(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync("c1");
    });

    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe("/api/settings/default_connection_id");
    expect((init as RequestInit).method).toBe("PUT");
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ value: "c1" });
    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["settings"] });
    expect(removeSpy).not.toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["connections"] }));
    expect(qc.getMutationCache().getAll()[0].options.mutationKey).toEqual(["connection"]);
  });

  it("useClearDefaultConnection sends DELETE /settings/default_connection_id, removes settings and carries mutationKey", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const removeSpy = vi.spyOn(qc, "removeQueries");

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method || "GET").toUpperCase();
      if (url.includes("/api/settings/default_connection_id") && method === "DELETE") {
        return json({ value: null, is_default: true });
      }
      return json({}, 404);
    });
    vi.stubGlobal("fetch", fetchMock);

    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);

    const { result } = renderHook(() => useClearDefaultConnection(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync();
    });

    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe("/api/settings/default_connection_id");
    expect((init as RequestInit).method).toBe("DELETE");
    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["settings"] });
    expect(removeSpy).not.toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["connections"] }));
    expect(qc.getMutationCache().getAll()[0].options.mutationKey).toEqual(["connection"]);
  });
});
