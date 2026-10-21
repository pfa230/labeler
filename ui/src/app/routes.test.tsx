import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "./toast";
import { routes } from "./routes";

type AuthState = { authed: boolean; needsSetup: boolean; me?: { id: string; username: string } };

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

const viewTemplates = ["Warehouse", "Shipping"].map((category) => ({
  id: category.toLowerCase(), name: `${category} bin`, categories: [category],
  description: "", unit: "mm", dpi: 300, format: { type: "single", width: 80, height: 24 },
}));

function stubFetch(state: AuthState, withTemplates: boolean) {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/auth/me")) return json(state);
    if (url === "/api/activity") return json({ entries: [], next_before_id: null });
    if (url === "/api/templates") return json({ templates: withTemplates ? viewTemplates : [] });
    if (url.startsWith("/api/templates/")) {
      return json({ ...viewTemplates.find((t) => url.endsWith(t.id)), params: [{ name: "sku", type: "string", control: "text" }] });
    }
    if (url === "/api/printers") return json([]);
    if (url.startsWith("/api/favorites")) return json([]);
    if (url.startsWith("/api/recent-templates")) return json([]);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

// Builds the router from the shared `routes` the application ships, so this exercises the real
// destinations, guards and fallback rather than a copied tree.
function renderApp(state: AuthState, initialEntries: string[], withTemplates = false) {
  vi.stubGlobal("fetch", stubFetch(state, withTemplates));
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

  it("restores Labels through Back and shell navigation, with explicit URLs taking precedence", async () => {
    const router = renderApp(authed, ["/"], true);
    fireEvent.click(await screen.findByRole("button", { name: "Warehouse" }));
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "bin" } });
    expect(router.state.location.search).toBe("?category=Warehouse&q=bin");
    fireEvent.click(screen.getByRole("link", { name: "Import" }));
    await screen.findByRole("heading", { name: "Import" });
    await act(() => router.navigate(-1));
    expect(await screen.findByRole("searchbox")).toHaveValue("bin");
    expect(screen.getByRole("button", { name: "Warehouse" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("link", { name: "Print Shipping bin" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("link", { name: "Import" }));
    await screen.findByRole("heading", { name: "Import" });
    expect(screen.getByRole("link", { name: "Labels" })).toHaveAttribute("href", "/?category=Warehouse&q=bin");
    fireEvent.click(screen.getByRole("link", { name: "Labels" }));
    expect(await screen.findByRole("searchbox")).toHaveValue("bin");
    await act(() => router.navigate("/?category=Shipping"));
    expect(screen.getByRole("searchbox")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Shipping" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("link", { name: "Print Warehouse bin" })).not.toBeInTheDocument();
    await act(() => router.navigate("/"));
    expect(screen.getByRole("button", { name: "All" })).toHaveAttribute("aria-pressed", "true");
  });

  it("uses Import's URL on Back and direct links, and remembers the shell destination", async () => {
    const router = renderApp(authed, ["/import?template=warehouse"], true);
    await screen.findByRole("option", { name: "Warehouse bin" });
    expect(screen.getByLabelText("template")).toHaveValue("warehouse");
    fireEvent.change(screen.getByLabelText("template"), { target: { value: "shipping" } });
    expect(router.state.location.search).toBe("?template=shipping");
    fireEvent.change(screen.getByLabelText("paste CSV"), { target: { value: "sku\noriginal" } });
    fireEvent.click(screen.getByRole("button", { name: "Load CSV" }));
    fireEvent.change(await screen.findByLabelText("edit sku"), { target: { value: "edited in tab" } });
    fireEvent.click(screen.getByRole("link", { name: "Labels" }));
    await screen.findByRole("heading", { name: "Labels" });
    expect(screen.getByRole("link", { name: "Import" })).toHaveAttribute("href", "/import?template=shipping");
    await act(() => router.navigate(-1));
    expect(await screen.findByLabelText("template")).toHaveValue("shipping");
    expect(await screen.findByLabelText("edit sku")).toHaveValue("edited in tab");
    await act(() => router.navigate("/import?template=warehouse"));
    expect(screen.getByLabelText("template")).toHaveValue("warehouse");
    expect(screen.getByLabelText("edit sku")).toHaveValue("edited in tab");
    await act(() => router.navigate("/import"));
    expect(screen.getByLabelText("template")).toHaveValue("");
  });

  it("serves Activity through the authenticated shell", async () => {
    const router = renderApp(authed, ["/activity"]);
    await screen.findByRole("heading", { name: "Activity" });
    expect(router.state.location.pathname).toBe("/activity");
    expect(await screen.findByText("No activity recorded yet.")).toBeInTheDocument();
  });

  it("guards Activity for anonymous visitors", async () => {
    const router = renderApp({ authed: false, needsSetup: false }, ["/activity"]);
    await screen.findByRole("heading", { name: /sign in/i });
    expect(router.state.location.pathname).toBe("/login");
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
