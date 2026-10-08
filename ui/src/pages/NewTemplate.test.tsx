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

function typeAndCreate(id: string, yaml: string) {
  fireEvent.change(screen.getByLabelText(/template id/i), { target: { value: id } });
  fireEvent.change(screen.getByLabelText(/template yaml/i), { target: { value: yaml } });
  fireEvent.click(screen.getByRole("button", { name: /create/i }));
}

describe("New template", () => {
  beforeEach(() => vi.unstubAllGlobals());

  it("creates with a POST of the raw YAML and navigates to the created template", async () => {
    const fetchMock = vi.fn<typeof fetch>(
      async () =>
        new Response(JSON.stringify({ id: "new-tpl" }), {
          status: 201,
          headers: { "content-type": "application/json" },
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    typeAndCreate("new-tpl", "name: New Template\n");
    expect(await screen.findByText(/detail for/i)).toBeInTheDocument();
    const writes = fetchMock.mock.calls.filter(([, init]) => init?.method !== undefined);
    expect(writes).toHaveLength(1);
    const [url, init] = writes[0];
    expect(url).toBe("/api/templates/new-tpl");
    expect(init?.method).toBe("POST");
    expect(init?.body).toBe("name: New Template\n");
    expect(new Headers(init?.headers).has("if-none-match")).toBe(false);
  });

  it("shows the already-exists message inline on a 409", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({ error: { code: "Conflict", message: "conflict" } }),
            { status: 409, headers: { "content-type": "application/json" } },
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
