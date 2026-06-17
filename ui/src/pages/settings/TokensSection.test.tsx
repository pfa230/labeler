import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { TokensSection } from "./TokensSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type T = { id: string; name: string; last_used_at: string | null; created_at: string };

// Stateful stub: POST returns the secret ONCE; the listed tokens never carry a secret. DELETE revokes.
function stubFetch() {
  let state: T[] = [{ id: "t1", name: "ci", last_used_at: null, created_at: "2026-01-01" }];
  let seq = 0;
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/tokens/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/tokens/".length));
      state = state.filter((t) => t.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/tokens") && method === "POST") {
      const body = JSON.parse(init!.body as string) as { name: string };
      const id = `t-${++seq}`;
      state = [...state, { id, name: body.name, last_used_at: null, created_at: "2026-02-02" }];
      return json({ id, name: body.name, secret: `lbl_secret_${seq}` }, 201);
    }
    if (url.startsWith("/api/tokens")) return json(state);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <TokensSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string, method: string) =>
  [...fetchMock.mock.calls].reverse().find(([u, i]) => String(u).startsWith(path) && ((i as RequestInit)?.method ?? "GET").toUpperCase() === method);

describe("TokensSection", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("lists tokens without ever showing a secret", async () => {
    renderSection();
    expect(await screen.findByText("ci")).toBeInTheDocument();
    expect(screen.queryByLabelText("token secret")).not.toBeInTheDocument();
  });

  it("creates a token and shows the secret once", async () => {
    renderSection();
    await screen.findByText("ci");
    fireEvent.change(screen.getByLabelText(/token name/i), { target: { value: "deploy" } });
    fireEvent.click(screen.getByRole("button", { name: /create token/i }));
    await waitFor(() => expect(lastCall("/api/tokens", "POST")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/tokens", "POST")![1] as RequestInit).body as string);
    expect(body).toEqual({ name: "deploy" });
    // the secret is shown once, with the "will not see again" warning
    const secret = await screen.findByLabelText("token secret");
    expect(secret).toHaveTextContent("lbl_secret_1");
    expect(screen.getByText(/will not see this secret again/i)).toBeInTheDocument();
    // the new token appears in the refetched table (without a secret column)
    expect(await screen.findByText("deploy")).toBeInTheDocument();
    // dismissing hides the secret permanently
    fireEvent.click(screen.getByRole("button", { name: /done/i }));
    expect(screen.queryByLabelText("token secret")).not.toBeInTheDocument();
  });

  it("revokes a token via DELETE after a confirm", async () => {
    renderSection();
    const row = (await screen.findByText("ci")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /^revoke$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    await waitFor(() => expect(lastCall("/api/tokens/t1", "DELETE")).toBeTruthy());
    await waitFor(() => expect(screen.queryByText("ci")).not.toBeInTheDocument());
  });
});
