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

function typeAndCreate(id: string, yaml: string, group?: string) {
  fireEvent.change(screen.getByLabelText(/template id/i), { target: { value: id } });
  if (group) {
    fireEvent.change(screen.getByLabelText(/template group/i), { target: { value: group } });
  }
  fireEvent.change(screen.getByLabelText(/template yaml/i), { target: { value: yaml } });
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
    typeAndCreate("new-tpl", "name: New Template\n");
    expect(await screen.findByText(/detail for/i)).toBeInTheDocument();
  });

  it("shows the error message inline on a 412 conflict", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({ error: { code: "PreconditionFailed", message: "already exists" } }),
            { status: 412, headers: { "content-type": "application/json" } },
          ),
      ),
    );
    renderPage();
    typeAndCreate("existing-tpl", "name: Existing\n");
    const matches = await screen.findAllByText("A template with ID 'existing-tpl' already exists");
    expect(matches.some((el) => el.tagName === "P")).toBe(true);
  });

  it("shows the error message inline on a 422", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({ error: { code: "TemplateInvalid", message: "invalid unit: foo" } }),
            { status: 422, headers: { "content-type": "application/json" } },
          ),
      ),
    );
    renderPage();
    typeAndCreate("bad-tpl", "unit: foo\n");
    const matches = await screen.findAllByText("invalid unit: foo");
    expect(matches.some((el) => el.tagName === "P")).toBe(true);
  });
});
