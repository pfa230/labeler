import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { createMemoryRouter, Outlet, RouterProvider, useNavigate } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { NewTemplate } from "./NewTemplate";

function GoToSettings() {
  const navigate = useNavigate();
  return (
    <button type="button" onClick={() => navigate("/settings")}>
      go to settings
    </button>
  );
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // A data router, not a declarative MemoryRouter: the YAML guard's useBlocker needs its context.
  const router = createMemoryRouter(
    [
      {
        element: (
          <>
            <GoToSettings />
            <Outlet />
          </>
        ),
        children: [
          { path: "/templates/new", element: <NewTemplate /> },
          { path: "/templates/:id", element: <div>detail page</div> },
          { path: "/settings", element: <div>settings page</div> },
        ],
      },
    ],
    { initialEntries: ["/templates/new"] },
  );
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <RouterProvider router={router} />
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
    // A successful create makes the submitted YAML saved, so the guard must not block the move.
    expect(await screen.findByText(/detail page/i)).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
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

  it("blocks navigation with unsaved YAML, then reaches the destination on Discard", async () => {
    renderPage();
    fireEvent.change(screen.getByLabelText(/template yaml/i), { target: { value: "name: Draft\n" } });

    fireEvent.click(screen.getByRole("button", { name: /go to settings/i }));
    expect(await screen.findByRole("alertdialog", { name: /unsaved yaml changes/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));

    // Stayed on the editor with the draft intact.
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByLabelText(/template yaml/i)).toHaveValue("name: Draft\n");

    fireEvent.click(screen.getByRole("button", { name: /go to settings/i }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    expect(await screen.findByText("settings page")).toBeInTheDocument();
  });

  it("does not prompt when the YAML is unchanged", async () => {
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: /go to settings/i }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(await screen.findByText("settings page")).toBeInTheDocument();
  });

  it("does not prompt when only the ID changes", async () => {
    renderPage();
    fireEvent.change(screen.getByLabelText(/template id/i), { target: { value: "my-label" } });
    fireEvent.click(screen.getByRole("button", { name: /go to settings/i }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(await screen.findByText("settings page")).toBeInTheDocument();
  });

  it("keeps protecting the draft after a failed create", async () => {
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
    await screen.findAllByText("invalid unit: foo");

    fireEvent.click(screen.getByRole("button", { name: /go to settings/i }));
    expect(await screen.findByRole("alertdialog", { name: /unsaved yaml changes/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
    expect(screen.getByLabelText(/template yaml/i)).toHaveValue("unit: foo\n");
  });
});
