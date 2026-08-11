import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { TemplateDetail } from "./TemplateDetail";

const detail = {
  id: "brother_24mm_qr",
  name: "Brother 24mm Continuous Label",
  description: "Continuous label roll (24mm width)",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: { min: 10, max: 120 }, height: 24 },
  layout: [
    { type: "qr", name: "code" },
    { type: "text", name: "message" },
  ],
};

const source = "id: brother_24mm_qr\nname: Brother 24mm Continuous Label\n";

function stubFetch(deleteStatus = 204) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (init?.method === "DELETE") {
        return deleteStatus === 204
          ? new Response(null, { status: 204 })
          : new Response(
              JSON.stringify({ error: { code: "RenderFailed", message: "delete failed" } }),
              { status: deleteStatus, headers: { "content-type": "application/json" } },
            );
      }
      if (url.endsWith("/api/templates/brother_24mm_qr/source")) {
        return new Response(source, { status: 200, headers: { "content-type": "text/yaml" } });
      }
      if (url.endsWith("/api/templates/brother_24mm_qr")) {
        return new Response(JSON.stringify(detail), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.endsWith("/api/render/label")) {
        return new Response(new Blob(["x"]), {
          status: 200,
          headers: { "content-type": "image/png" },
        });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }),
  );
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={["/templates/brother_24mm_qr"]}>
          <Routes>
            <Route path="/templates/:id" element={<TemplateDetail />} />
            {/* The list is the app's index route; "/templates" is only a redirect to it. */}
            <Route path="/" element={<div>Labels list</div>} />
          </Routes>
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Template detail", () => {
  // Wrapped, not passed by reference: beforeEach hands the hook a test context, which would land in
  // stubFetch's deleteStatus parameter.
  beforeEach(() => stubFetch());

  it("renders name, referenced fields, format badge, and a use-to-print link", async () => {
    renderPage();
    expect(await screen.findByText("Brother 24mm Continuous Label")).toBeInTheDocument();
    expect(screen.getByText("message")).toBeInTheDocument();
    expect(screen.getByText("code")).toBeInTheDocument();
    expect(screen.getByText("single")).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /use to print/i });
    expect(link).toHaveAttribute("href", "/print/brother_24mm_qr");
  });

  it("reveals the raw YAML source when toggled", async () => {
    renderPage();
    await screen.findByText("Brother 24mm Continuous Label");
    const toggle = await screen.findByText(/raw yaml/i);
    fireEvent.click(toggle);
    expect(await screen.findByText(/id: brother_24mm_qr/)).toBeInTheDocument();
  });

  it("deletes after confirming, then returns to the list", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    expect(await screen.findByText("Labels list")).toBeInTheDocument();
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    expect(
      calls.some(
        ([url, init]) =>
          String(url).endsWith("/api/templates/brother_24mm_qr") && init?.method === "DELETE",
      ),
    ).toBe(true);
    expect(await screen.findByText(/Deleted brother_24mm_qr/)).toBeInTheDocument();
  });

  it("cancelling sends no request", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls.some(([, init]) => init?.method === "DELETE")).toBe(false);
  });

  it("keeps the user on the page when the delete fails", async () => {
    stubFetch(500);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    expect(await screen.findByText(/delete failed/i)).toBeInTheDocument();
    expect(screen.queryByText("Labels list")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
  });
});
