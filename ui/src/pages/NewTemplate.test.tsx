import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { NewTemplate } from "./NewTemplate";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={["/templates/new"]}>
          <Routes>
            <Route path="/templates/new" element={<NewTemplate />} />
            <Route path="/templates/:id" element={<div>detail for {window.location.pathname}</div>} />
          </Routes>
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

function typeAndCreate(yaml: string) {
  fireEvent.change(screen.getByRole("textbox"), { target: { value: yaml } });
  fireEvent.click(screen.getByRole("button", { name: /create/i }));
}

describe("New template", () => {
  beforeEach(() => vi.unstubAllGlobals());

  it("navigates to the created template on success", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ id: "new-tpl" }), {
            status: 201,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
    renderPage();
    typeAndCreate("id: new-tpl");
    expect(await screen.findByText(/detail for/i)).toBeInTheDocument();
  });

  it("shows the error message inline on a 422", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({ error: { code: "TemplateInvalid", message: "missing field: id" } }),
            { status: 422, headers: { "content-type": "application/json" } },
          ),
      ),
    );
    renderPage();
    typeAndCreate("bad: yaml");
    const matches = await screen.findAllByText("missing field: id");
    expect(matches.some((el) => el.tagName === "P")).toBe(true);
  });
});
