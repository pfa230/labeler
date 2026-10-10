import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Print } from "./Print";

const detail = {
  id: "t1",
  name: "Tag",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  params: [{ name: "message", type: "string", control: "text" }],
};

const detail2 = {
  id: "t2",
  name: "Card",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  params: [{ name: "message", type: "string", control: "text" }],
};

const list = {
  templates: [
    { id: "t1", name: "Tag", description: "", unit: "mm", dpi: 300, format: detail.format },
    { id: "t2", name: "Card", description: "", unit: "mm", dpi: 300, format: detail2.format },
  ],
};
// Two printers with no default, so the one-shot preselect falls through to "none"
// (it only auto-picks a lone printer or an explicit default) and Print stays gated on an
// explicit printer selection — which is what this suite exercises.
const printers = [
  { id: "p1", name: "Label Printer", uri: "ipp://p1/q", insecure: false },
  { id: "p2", name: "Backup Printer", uri: "ipp://p2/q", insecure: false },
];
const summary = { total: 1, sent: 1, failed: [], jobs: 1 };

function stubFetch() {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    // Detail BEFORE list so the broad /api/templates branch doesn't swallow it.
    if (url.startsWith("/api/templates/nope")) {
      return new Response(
        JSON.stringify({ error: { code: "NotFound", message: "template not found" } }),
        { status: 404, headers: { "content-type": "application/json" } },
      );
    }
    if (url.startsWith("/api/templates/t1")) {
      return new Response(JSON.stringify(detail), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.startsWith("/api/templates/t2")) {
      return new Response(JSON.stringify(detail2), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.startsWith("/api/templates")) {
      return new Response(JSON.stringify(list), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.startsWith("/api/printers")) {
      return new Response(JSON.stringify(printers), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url === "/api/settings") {
      return new Response(JSON.stringify({ default_printer_id: { value: null, is_default: true } }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (url.startsWith("/api/render/label")) {
      return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
    }
    if (url === "/api/print") {
      void init;
      return new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url === "/api/render") {
      return new Response(new Blob(["PK"]), {
        status: 200,
        headers: { "content-type": "application/zip", "content-disposition": 'attachment; filename="t1.zip"' },
      });
    }
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderWithProviders(
  ui: React.ReactElement,
  options?: { template?: { id: string; [key: string]: unknown }; initialPath?: string },
) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const template = options?.template;
  const initialPath = options?.initialPath ?? (template ? `/print/${template.id}` : "/print");

  if (template) {
    const currentStub = fetchMock;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.startsWith(`/api/templates/${template.id}`)) {
          return new Response(JSON.stringify(template), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }
        return currentStub(input, init);
      }),
    );
  }

  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={[initialPath]}>
          <Routes>
            <Route path="/" element={<div>labels grid</div>} />
            <Route path="/print" element={ui} />
            <Route path="/print/:templateId" element={ui} />
          </Routes>
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

function renderPage(initialPath = "/print") {
  return renderWithProviders(<Print />, { initialPath });
}

let fetchMock: ReturnType<typeof stubFetch>;
const countCalls = (path: string) => fetchMock.mock.calls.filter(([u]) => String(u).startsWith(path)).length;

describe("Print screen", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders a control per published parameter: textarea, number, checkbox and select", async () => {
    const templateWithParams = {
      id: "test_tpl",
      name: "Test Template",
      description: "",
      unit: "mm",
      dpi: 300,
      params: [
        { name: "message", type: "string", control: "text", description: "Single line" },
        { name: "notes", type: "string", control: "textarea", multiline: true, default: "", description: "Notes" },
        { name: "target_width", type: "length", control: "number", default: 80, min: 25, max: 200, description: "Target width" },
        { name: "show_border", type: "boolean", control: "checkbox", default: false, description: "Show border" },
        { name: "orientation", type: "enum", control: "select", values: ["horizontal", "vertical"], default: "horizontal", description: "orientation" },
      ],
      format: { type: "single" as const, height: 18, width: { min: 25, max: 80 } },
    };

    renderWithProviders(<Print />, { template: templateWithParams });
    expect(await screen.findByRole("textbox", { name: /notes/i })).toBeInstanceOf(HTMLTextAreaElement);
    expect(screen.getByRole("spinbutton", { name: /target width/i })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /show border/i })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: /orientation/i })).toBeInTheDocument();
  });

  it("redirects /print (no id) to the grid", async () => {
    renderPage("/print");
    expect(await screen.findByText("labels grid")).toBeInTheDocument();
  });

  it("gates Print on a printer, not on a filled field, then prints", async () => {
    const createUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderPage("/print/t1");

    // The message field appears once the detail loads.
    const message = (await screen.findByLabelText("message")) as HTMLInputElement;

    // The screen does not judge completeness: Download is enabled with the field blank, and only
    // the missing printer gates Print.
    const download = screen.getByRole("button", { name: /download/i });
    const print = screen.getByRole("button", { name: /print/i });
    expect(download).toBeEnabled();
    expect(print).toBeDisabled();

    fireEvent.change(message, { target: { value: "hello" } });

    // Let the live preview settle so we can assert on the download delta.
    await waitFor(() => expect(countCalls("/api/render/label")).toBeGreaterThan(0));
    const beforeUrls = createUrl.mock.calls.length;

    // A single template downloads through /api/render: one label per copy, in the chosen format.
    const renderCall = () => [...fetchMock.mock.calls].reverse().find(([u]) => String(u) === "/api/render");
    fireEvent.click(download);
    await waitFor(() => expect(renderCall()).toBeDefined());
    await waitFor(() => expect(createUrl.mock.calls.length).toBe(beforeUrls + 1));
    const renderBody = JSON.parse((renderCall()![1] as RequestInit).body as string);
    expect(renderBody).toEqual({ template: "t1", labels: [{ data: { message: "hello" } }], format: "png" });

    // Select the printer → Print enables.
    fireEvent.change(screen.getByLabelText("printer"), { target: { value: "p1" } });
    await waitFor(() => expect(print).not.toBeDisabled());

    const printCall = () => [...fetchMock.mock.calls].reverse().find(([u]) => String(u) === "/api/print");
    fireEvent.click(print);
    await waitFor(() => expect(printCall()).toBeDefined());
    const printBody = JSON.parse((printCall()![1] as RequestInit).body as string);
    expect(printBody).toEqual({ template: "t1", printer: "p1", labels: [{ data: { message: "hello" } }] });
    expect(await screen.findByText("Sent 1 labels to Label Printer")).toBeInTheDocument();
  });

  it("renders the form for a template from the URL param", async () => {
    renderPage("/print/t1");
    expect(await screen.findByLabelText("message")).toBeInTheDocument();
  });

  it("shows an error and the all-labels link for an unknown id", async () => {
    renderPage("/print/nope");
    expect(await screen.findByText(/template not found/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /all labels/i })).toHaveAttribute("href", "/");
  });
});
