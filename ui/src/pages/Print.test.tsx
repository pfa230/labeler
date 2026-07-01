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
  layout: [{ type: "text", name: "message" }],
};

const detail2 = {
  id: "t2",
  name: "Card",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  layout: [{ type: "text", name: "message" }],
};

const list = {
  templates: [
    { id: "t1", name: "Tag", description: "", unit: "mm", dpi: 300, format: detail.format },
    { id: "t2", name: "Card", description: "", unit: "mm", dpi: 300, format: detail2.format },
  ],
};
// Two enabled printers with no default, so the one-shot preselect falls through to "none"
// (it only auto-picks a lone enabled printer or an explicit default) and Print stays gated on an
// explicit printer selection — which is what this suite exercises.
const printers = [
  { id: "p1", name: "Label Printer", kind: "cups", config: null, enabled: true },
  { id: "p2", name: "Backup Printer", kind: "cups", config: null, enabled: true },
];
const summary = { total: 1, succeeded: 1, failed: [], jobs: 1 };

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

function renderPage(initialPath = "/print") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={[initialPath]}>
          <Routes>
            <Route path="/print" element={<Print />} />
            <Route path="/print/:templateId" element={<Print />} />
          </Routes>
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
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

  it("shows an empty state until a template is chosen", async () => {
    renderPage();
    expect(await screen.findByText(/choose a template/i)).toBeInTheDocument();
  });

  it("gates Download on a filled field and Print on a printer, then prints", async () => {
    const createUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderPage();

    // Select t1 in the picker once the list has loaded its option.
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });

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

  it("shows the empty state at /print with no id", async () => {
    renderPage("/print");
    expect(await screen.findByText(/choose a template/i)).toBeInTheDocument();
  });

  it("shows an error and keeps the picker for an unknown id", async () => {
    renderPage("/print/nope");
    expect(await screen.findByText(/template not found/i)).toBeInTheDocument();
    expect(screen.getByLabelText("template")).toBeInTheDocument();
  });

  it("navigates to /print/{id} when a template is chosen", async () => {
    renderPage("/print");
    const picker = await screen.findByLabelText("template");
    // Wait for the option to load; a controlled <select> ignores a value whose option is absent.
    await screen.findByRole("option", { name: "Card" });
    fireEvent.change(picker, { target: { value: "t2" } });
    expect(await screen.findByLabelText("message")).toBeInTheDocument();
  });

  it("keeps entered fields when switching to a template sharing the field", async () => {
    renderPage();

    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });

    const message = (await screen.findByLabelText("message")) as HTMLInputElement;
    fireEvent.change(message, { target: { value: "hello" } });
    expect(message.value).toBe("hello");

    // Switch to t2, which also references "message"; the value must survive (no remount wipe).
    fireEvent.change(picker, { target: { value: "t2" } });
    const message2 = (await screen.findByLabelText("message")) as HTMLInputElement;
    expect(message2.value).toBe("hello");
  });
});
