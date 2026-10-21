import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "./toast";
import { Shell } from "./Shell";
import { App } from "./App";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

// Stateful stub for /api/auth/me + /api/auth/logout so Shell's useAuth/useLogout resolve.
function stubFetch() {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/auth/logout") && method === "POST") return json({ ok: true });
    if (url.startsWith("/api/auth/me")) return json({ authed: true, needsSetup: false, me: { id: "u1", username: "alice" } });
    throw new Error(`unexpected fetch: ${url}`);
  });
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string, method: string) =>
  [...fetchMock.mock.calls].reverse().find(([u, i]) => String(u).startsWith(path) && ((i as RequestInit)?.method ?? "GET").toUpperCase() === method);

function renderShell() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <MemoryRouter><Shell /></MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Shell", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders the four nav sections in order", () => {
    renderShell();
    const nav = screen.getByRole("navigation", { name: "Primary" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((l) => l.textContent?.trim())).toEqual([
      "Labels",
      "Import",
      "Print history",
      "Activity",
      "Settings",
    ]);
    expect(links.map((l) => l.getAttribute("href"))).toEqual([
      "/",
      "/import",
      "/print-history",
      "/activity",
      "/settings",
    ]);

    expect(screen.queryByRole("link", { name: "Templates" })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Print" })).not.toBeInTheDocument();
  });

  it("navigates to the print history route from primary navigation", async () => {
    window.history.pushState({}, "", "/");
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.startsWith("/api/auth/me"))
          return json({ authed: true, needsSetup: false, noAuth: false, me: { id: "u1", username: "alice" } });
        if (url.startsWith("/api/templates")) return json({ templates: [] });
        if (url.startsWith("/api/favorites")) return json([]);
        if (url.startsWith("/api/recent-templates")) return json([]);
        if (url.startsWith("/api/jobs")) return json([]);
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <App />
        </ToastProvider>
      </QueryClientProvider>,
    );

    const nav = await screen.findByRole("navigation", { name: "Primary" });
    fireEvent.click(within(nav).getByRole("link", { name: "Print history" }));
    expect(await screen.findByRole("heading", { level: 1, name: /print history/i })).toBeInTheDocument();
    expect(window.location.pathname).toBe("/print-history");
  });

  it("shows the current username and logs out via POST", async () => {
    renderShell();
    expect(await screen.findByText("alice")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /logout/i }));
    await waitFor(() => expect(lastCall("/api/auth/logout", "POST")).toBeTruthy());
  });
});
