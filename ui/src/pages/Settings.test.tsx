import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Settings } from "./Settings";

const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });

function stubFetch() {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/variables")) return json({ qr_base_url: "https://x" });
    if (url.startsWith("/api/printers")) return json([]);
    if (url.startsWith("/api/connections")) return json([]);
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
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.stubGlobal("fetch", stubFetch());
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders both sections", async () => {
    renderPage();
    expect(await screen.findByRole("heading", { level: 1, name: /^settings$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /printers/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^variables$/i })).toBeInTheDocument();
    // variables section loaded the stored qr_base_url value
    expect(await screen.findByLabelText("qr_base_url")).toHaveValue("https://x");
    // printers empty state
    expect(await screen.findByText(/no printers configured/i)).toBeInTheDocument();
  });
});
