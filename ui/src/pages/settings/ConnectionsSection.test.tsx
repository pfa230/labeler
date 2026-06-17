import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { ConnectionsSection } from "./ConnectionsSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type C = { id: string; connector: string; name: string; base_url: string; enabled: boolean; has_credential: boolean };
type ConnectionInputBody = { connector: string; name: string; base_url: string; credential?: string };

function stubFetch() {
  let state: C[] = [];
  return vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input, init) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/connections/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      state = state.filter((c) => c.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/connections") && method === "POST") {
      const b = JSON.parse(init!.body as string) as ConnectionInputBody;
      const c: C = { id: "id1", connector: b.connector, name: b.name, base_url: b.base_url, enabled: true, has_credential: !!b.credential };
      state = [...state, c];
      return json(c, 201);
    }
    if (url.startsWith("/api/connections")) return json(state);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <ConnectionsSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
describe("ConnectionsSection", () => {
  beforeEach(() => { vi.unstubAllGlobals(); fetchMock = stubFetch(); vi.stubGlobal("fetch", fetchMock); });
  afterEach(() => vi.unstubAllGlobals());

  it("creates a connection and never displays the credential", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();
    expect(screen.queryByText("hb_secret")).not.toBeInTheDocument();
    const post = fetchMock.mock.calls.find(([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST");
    expect(JSON.parse((post![1]!.body) as string).credential).toBe("hb_secret");
  });

  it("requires an api key when creating", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText(/api key is required/i)).toBeInTheDocument();
  });
});
