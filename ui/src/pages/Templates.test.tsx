import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Templates } from "./Templates";

const templates = [
  {
    id: "brother_24mm_qr",
    name: "Brother 24mm",
    description: "Continuous label roll",
    unit: "mm",
    dpi: 300,
    format: { type: "single", width: 80, height: 24 },
  },
  {
    id: "avery5163",
    name: "Avery 5163",
    description: "Shipping labels",
    unit: "in",
    dpi: 300,
    format: {
      type: "sheet",
      paper_width: 8.5,
      paper_height: 11,
      label_width: 4,
      label_height: 2,
      positions: [[0, 0]],
    },
  },
];

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter>
          <Templates />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Templates list", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ templates }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("renders both names and their format badges", async () => {
    renderPage();
    expect(await screen.findByText("Brother 24mm")).toBeInTheDocument();
    expect(screen.getByText("Avery 5163")).toBeInTheDocument();
    expect(screen.getByText("single")).toBeInTheDocument();
    expect(screen.getByText("sheet")).toBeInTheDocument();
  });

  it("card main link goes to the print form; details link to the template page", async () => {
    renderPage();
    // The card link gets aria-label "Print {name}" so queries are unambiguous vs the details link
    // (a bare /brother 24mm/i regex would match BOTH links' accessible names).
    const card = await screen.findByRole("link", { name: "Print Brother 24mm" });
    expect(card).toHaveAttribute("href", "/print/brother_24mm_qr");
    const details = screen.getByRole("link", { name: "Brother 24mm template details" });
    expect(details).toHaveAttribute("href", "/templates/brother_24mm_qr");
  });

  it("filters cards by name from the search box", async () => {
    renderPage();
    await screen.findByRole("link", { name: "Print Brother 24mm" });
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "avery" } });
    expect(screen.queryByRole("link", { name: "Print Brother 24mm" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Print Avery 5163" })).toBeInTheDocument();
  });

  it("shows the Labels heading", async () => {
    renderPage();
    expect(await screen.findByRole("heading", { name: "Labels" })).toBeInTheDocument();
  });

  it("filters cards by id from the search box", async () => {
    renderPage();
    await screen.findByText("Brother 24mm");
    const search = screen.getByRole("searchbox");
    fireEvent.change(search, { target: { value: "avery" } });
    expect(screen.getByText("Avery 5163")).toBeInTheDocument();
    expect(screen.queryByText("Brother 24mm")).not.toBeInTheDocument();
  });

  it("renders a thumbnail image per card pointing at the thumbnail endpoint", async () => {
    renderPage();
    const img = await screen.findByAltText("Brother 24mm preview");
    expect(img).toHaveAttribute("src", "/api/templates/brother_24mm_qr/thumbnail");
    expect(img.tagName).toBe("IMG");
  });

  it("falls back to a placeholder when the thumbnail image fails to load", async () => {
    renderPage();
    const img = await screen.findByAltText("Avery 5163 preview");
    fireEvent.error(img);
    expect(screen.getByText("preview", { selector: "div" })).toBeInTheDocument();
  });
});
