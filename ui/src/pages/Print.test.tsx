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
  inputs: {
    all: [{ name: "message", control: "text" }],
    default: [{ name: "message", control: "text" }],
  },
};

const detail2 = {
  id: "t2",
  name: "Card",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  inputs: {
    all: [{ name: "message", control: "text" }],
    default: [{ name: "message", control: "text" }],
  },
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
  { id: "p1", name: "Label Printer", kind: "cups", config: null },
  { id: "p2", name: "Backup Printer", kind: "cups", config: null },
];
const summary = { total: 1, succeeded: 1, failed: [], jobs: 1 };

function stubFetch() {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.includes("test_tpl/inputs")) {
      return new Response(
        JSON.stringify({
          inputs: [
            [
              { name: "message", control: "text", description: "Single line" },
              { name: "notes", control: "textarea", default: "", description: "Notes" },
              { name: "target_width", control: "number", slider: true, default: 80, min: 25, max: 200, description: "Target width" },
              { name: "show_border", control: "checkbox", default: false, description: "Show border" },
              { name: "orientation", control: "select", values: ["horizontal", "vertical"], default: "horizontal", description: "orientation" },
            ],
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (url.includes("/inputs")) {
      return new Response(
        JSON.stringify({ inputs: [[{ name: "message", control: "text" }]] }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
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
    if (url.startsWith("/api/render/label")) {
      return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
    }
    if (url === "/api/print") {
      void init;
      return new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.startsWith("/api/batch")) {
      void init;
      return new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } });
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
const lastCall = (path: string) =>
  [...fetchMock.mock.calls].reverse().find(([u]) => String(u).startsWith(path));
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

  it("renders dynamic inputs for multiline string, length slider, toggle, and enum", async () => {
    const templateWithParams = {
      id: "test_tpl",
      name: "Test Template",
      description: "",
      unit: "mm",
      dpi: 300,
      inputs: {
        all: [
          { name: "message", control: "text" as const, description: "Single line" },
          { name: "notes", control: "textarea" as const, default: "", description: "Notes" },
          { name: "target_width", control: "number" as const, slider: true, default: 80, min: 25, max: 200, description: "Target width" },
          { name: "show_border", control: "checkbox" as const, default: false, description: "Show border" },
          { name: "orientation", control: "select" as const, values: ["horizontal", "vertical"], default: "horizontal", description: "orientation" },
        ],
        default: [
          { name: "message", control: "text" as const, description: "Single line" },
          { name: "notes", control: "textarea" as const, default: "", description: "Notes" },
          { name: "target_width", control: "number" as const, slider: true, default: 80, min: 25, max: 200, description: "Target width" },
          { name: "show_border", control: "checkbox" as const, default: false, description: "Show border" },
          { name: "orientation", control: "select" as const, values: ["horizontal", "vertical"], default: "horizontal", description: "orientation" },
        ],
      },
      format: { type: "single" as const, height: 18, width: { min: 25, max: 80 } },
      layout: [],
    };

    renderWithProviders(<Print />, { template: templateWithParams });
    expect(await screen.findByRole("textbox", { name: /notes/i })).toBeInstanceOf(HTMLTextAreaElement);
    expect(screen.getByRole("slider", { name: /target width/i })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /show border/i })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: /orientation/i })).toBeInTheDocument();
  });

  it("redirects /print (no id) to the grid", async () => {
    renderPage("/print");
    expect(await screen.findByText("labels grid")).toBeInTheDocument();
  });

  it("gates Download on a filled field and Print on a printer, then prints", async () => {
    const createUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderPage("/print/t1");

    // The message field appears once the detail loads.
    const message = (await screen.findByLabelText("message")) as HTMLInputElement;

    const download = screen.getByRole("button", { name: /download/i });
    const print = screen.getByRole("button", { name: /print/i });
    expect(download).toBeDisabled();
    expect(print).toBeDisabled();

    // Fill the field: Download enables; Print stays disabled (no printer).
    fireEvent.change(message, { target: { value: "hello" } });
    await waitFor(() => expect(download).not.toBeDisabled());
    expect(print).toBeDisabled();

    // Let the live preview settle so we can assert on the download delta.
    await waitFor(() => expect(countCalls("/api/render/label")).toBeGreaterThan(0));
    const beforeRender = countCalls("/api/render/label");
    const beforeUrls = createUrl.mock.calls.length;

    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render/label")).toBe(beforeRender + 1));
    expect(createUrl.mock.calls.length).toBe(beforeUrls + 1);
    const lastRender = lastCall("/api/render/label")!;
    expect((lastRender[1] as RequestInit).method).toBe("POST");

    // Select the printer → Print enables.
    fireEvent.change(screen.getByLabelText("printer"), { target: { value: "p1" } });
    await waitFor(() => expect(print).not.toBeDisabled());

    // t1 is a single/tape template, so Print routes to /print (not /batch).
    const printCall = () => [...fetchMock.mock.calls].reverse().find(([u]) => String(u) === "/api/print");
    fireEvent.click(print);
    await waitFor(() => expect(printCall()).toBeDefined());
    const printBody = JSON.parse((printCall()![1] as RequestInit).body as string);
    expect(printBody.printer).toBe("p1");
    expect(printBody.copies).toBe(1);
    expect(await screen.findByText(/1\/1/)).toBeInTheDocument();
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
