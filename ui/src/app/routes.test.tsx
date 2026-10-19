import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "./toast";
import { routes } from "./routes";

type AuthState = { authed: boolean; needsSetup: boolean; me?: { id: string; username: string } };

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

function stubFetch(state: AuthState) {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/auth/me")) return json(state);
    if (url === "/api/templates") return json({ templates: [] });
    if (url.startsWith("/api/favorites")) return json([]);
    if (url.startsWith("/api/recent-templates")) return json([]);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

// Builds the router from the shared `routes` the application ships, so this exercises the real
// destinations, guards and fallback rather than a copied tree.
function renderApp(state: AuthState, initialEntries: string[]) {
  vi.stubGlobal("fetch", stubFetch(state));
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createMemoryRouter(routes, { initialEntries });
  render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <RouterProvider router={router} />
      </ToastProvider>
    </QueryClientProvider>,
  );
  return router;
}

const authed: AuthState = { authed: true, needsSetup: false, me: { id: "u1", username: "alice" } };

describe("App routes", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("redirects /templates to the index", async () => {
    const router = renderApp(authed, ["/templates"]);
    await screen.findByRole("heading", { name: "Labels" });
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
  });

  it("sends an unknown path to the index fallback", async () => {
    const router = renderApp(authed, ["/nowhere"]);
    await screen.findByRole("heading", { name: "Labels" });
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
  });

  it("sends an unauthenticated visitor to login", async () => {
    const router = renderApp({ authed: false, needsSetup: false }, ["/"]);
    await screen.findByRole("heading", { name: /sign in/i });
    await waitFor(() => expect(router.state.location.pathname).toBe("/login"));
  });

  it("sends a first-run visitor to setup", async () => {
    const router = renderApp({ authed: false, needsSetup: true }, ["/"]);
    await screen.findByRole("heading", { name: /create the first account/i });
    await waitFor(() => expect(router.state.location.pathname).toBe("/setup"));
  });

  it("sends an authed visitor away from /login", async () => {
    const router = renderApp(authed, ["/login"]);
    await screen.findByRole("heading", { name: "Labels" });
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
  });
});
