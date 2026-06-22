import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Settings } from "./Settings";

const json = (body: unknown) =>
  new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });

// Permissive stub: every section's query resolves; /auth/me carries the noAuth flag under test.
function stubFetch(noAuth: boolean) {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/auth/me"))
      return json({ authed: true, needsSetup: false, me: { id: "local", username: "local" }, noAuth });
    if (url.startsWith("/api/settings"))
      return json({ job_log_retention_days: { value: 90, is_default: true } });
    if (url.startsWith("/api/variables")) return json({ qr_base_url: "https://x" });
    if (url.startsWith("/api/printers")) return json([]);
    if (url.startsWith("/api/connections")) return json([]);
    if (url.startsWith("/api/users")) return json([]);
    if (url.startsWith("/api/tokens")) return json([]);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <Settings />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Settings page", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it("renders the core sections and credential sections when auth is on", async () => {
    vi.stubGlobal("fetch", stubFetch(false));
    renderPage();
    expect(await screen.findByRole("heading", { level: 1, name: /^settings$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /printers/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^variables$/i })).toBeInTheDocument();
    expect(await screen.findByLabelText("qr_base_url")).toHaveValue("https://x");
    expect(await screen.findByText(/no printers configured/i)).toBeInTheDocument();
    // credential sections present when auth is on
    expect(await screen.findByRole("heading", { name: "Users" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "API tokens" })).toBeInTheDocument();
  });

  it("hides Users and API tokens sections in no-auth mode", async () => {
    vi.stubGlobal("fetch", stubFetch(true));
    renderPage();
    // Variables always renders; wait for it so the page has settled before asserting absence
    await screen.findByRole("heading", { name: /^variables$/i });
    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "Users" })).not.toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: "API tokens" })).not.toBeInTheDocument();
    });
  });
});
