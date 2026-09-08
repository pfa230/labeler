import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "./toast";
import { Shell } from "./Shell";

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

  it("renders the five nav sections in order with Connections directly after Connect", () => {
    renderShell();
    const nav = screen.getByRole("navigation", { name: "Primary" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((l) => l.textContent?.trim())).toEqual([
      "Labels",
      "Import",
      "Connect",
      "Connections",
      "Settings",
    ]);
    expect(links.map((l) => l.getAttribute("href"))).toEqual([
      "/",
      "/import",
      "/connect",
      "/connections",
      "/settings",
    ]);

    const connectIdx = links.findIndex((l) => l.textContent?.trim() === "Connect");
    expect(links[connectIdx + 1]?.textContent?.trim()).toBe("Connections");

    expect(screen.queryByRole("link", { name: "Templates" })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Print" })).not.toBeInTheDocument();
  });

  it("shows the current username and logs out via POST", async () => {
    renderShell();
    expect(await screen.findByText("alice")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /logout/i }));
    await waitFor(() => expect(lastCall("/api/auth/logout", "POST")).toBeTruthy());
  });
});
