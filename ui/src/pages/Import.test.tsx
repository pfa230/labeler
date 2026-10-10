import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Import } from "./Import";

const detail = {
  id: "t1",
  name: "Tag",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  params: [
    { name: "sku", type: "string", control: "text" },
    { name: "color", type: "enum", control: "select", values: ["red", "blue"] },
  ],
};
const list = { templates: [{ id: "t1", name: "Tag", description: "", unit: "mm", dpi: 300, format: detail.format }] };
const printers = [{ id: "p1", name: "Label Printer", uri: "ipp://p1/q", insecure: false }];
const summary = { total: 2, sent: 2, failed: [], jobs: 1 };

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

// Optional `batch` override lets a test return a custom /api/render or /api/print response (failures, 422, etc.).
// Optional `renderLabel` override lets a test control the /api/render/label response.
function stubFetch(
  batch?: (body: Record<string, unknown>) => Response,
  renderLabel?: () => Response,
) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/templates/t1")) return json(detail);
    if (url.startsWith("/api/templates")) return json(list);
    if (url.startsWith("/api/printers")) return json(printers);
    if (url.startsWith("/api/render/label")) {
      if (renderLabel) return renderLabel();
      return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
    }
    if (url === "/api/render" || url === "/api/print") {
      const body = (init?.body ? JSON.parse(init.body as string) : {}) as Record<string, unknown>;
      if (batch) return batch(body);
      if (url === "/api/render") {
        return new Response(new Blob(["zip"]), { status: 200, headers: { "content-type": "application/zip" } });
      }
      return json(summary);
    }
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={["/import"]}>
          <Import />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const matches = (u: unknown, path: string) =>
  path === "/api/print" || path === "/api/render" ? String(u) === path : String(u).startsWith(path);
const lastCall = (path: string) => [...fetchMock.mock.calls].reverse().find(([u]) => matches(u, path));
const countCalls = (path: string) => fetchMock.mock.calls.filter(([u]) => matches(u, path)).length;

async function loadTemplateAndCsv() {
  const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
  await screen.findByRole("option", { name: "Tag" });
  fireEvent.change(picker, { target: { value: "t1" } });
  const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
  fireEvent.change(csv, { target: { value: "sku,color\n1,red\n2,blue\n" } });
  fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
  // The editor now renders before the template detail resolves; wait for detail-gated controls (copies)
  // so callers can interact with them synchronously.
  await screen.findByLabelText(/copies/i);
}

describe("CSV Import screen", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("loads a CSV into the grid and reports the expanded total", async () => {
    renderPage();
    await loadTemplateAndCsv();
    const grid = await screen.findByRole("grid", { name: /label rows/i });
    expect(within(grid).getByDisplayValue("1")).toBeInTheDocument();
    expect(within(grid).getByDisplayValue("2")).toBeInTheDocument();
    expect(screen.getByText(/2 labels/i)).toBeInTheDocument();
  });

  it("loads a CSV from a selected file", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const fileInput = (await screen.findByLabelText(/csv file/i)) as HTMLInputElement;
    const file = new File(["sku,color\n7,blue\n"], "labels.csv", { type: "text/csv" });
    fireEvent.change(fileInput, { target: { files: [file] } });
    expect(await screen.findByDisplayValue("7")).toBeInTheDocument();
  });

  it("loads a CSV dropped onto the dropzone", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const zone = await screen.findByLabelText(/csv dropzone/i);
    const file = new File(["sku,color\n8,red\n"], "labels.csv", { type: "text/csv" });
    fireEvent.drop(zone, { dataTransfer: { files: [file] } });
    expect(await screen.findByDisplayValue("8")).toBeInTheDocument();
  });

  it("posts a download batch for all resolved rows and saves the file", async () => {
    const createUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderPage();
    await loadTemplateAndCsv();
    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    expect(body.template).toBe("t1");
    expect(body.mode).toBeUndefined();
    expect(body.labels).toHaveLength(2);
    expect(body.labels[0]).toEqual({ data: { sku: "1", color: "red" } });
    // renderBatch read a binary blob and saved it via an object URL.
    await waitFor(() => expect(createUrl).toHaveBeenCalled());
    expect(body.start_slot).toBeUndefined(); // single template: start_slot omitted
  });

  it("submits CSV row data when the CSV omits optional columns", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n2\n" } }); // no color column
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByDisplayValue("1");
    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    expect(body.labels[0]).toEqual({ data: { sku: "1" } });
  });

  it("shows Print/Download in the action bar; Print is gated on a printer, Download is not", async () => {
    renderPage();
    await loadTemplateAndCsv();
    const print = await screen.findByRole("button", { name: /^print$/i });
    const download = screen.getByRole("button", { name: /^download$/i });
    // Both render; with no printer chosen, Print is disabled (gating) while Download stays enabled.
    expect(print).toBeInTheDocument();
    expect(print).toBeDisabled();
    expect(download).toBeEnabled();
    fireEvent.change(screen.getByLabelText(/printer/i), { target: { value: "p1" } });
    await waitFor(() => expect(print).toBeEnabled());
  });

  it("disables Run above the 500-label cap", async () => {
    renderPage();
    await loadTemplateAndCsv();
    const copies = screen.getByLabelText(/copies/i) as HTMLInputElement;
    fireEvent.change(copies, { target: { value: "300" } }); // 2 rows x 300 = 600 > 500
    await waitFor(() => expect(screen.getByRole("button", { name: /download/i })).toBeDisabled());
    expect(screen.getByText(/over the 500/i)).toBeInTheDocument();
  });

  it("prints and annotates rows from the summary", async () => {
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.change(screen.getByLabelText(/printer/i), { target: { value: "p1" } });
    fireEvent.click(await screen.findByRole("button", { name: /^print$/i }));
    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(body.template).toBe("t1");
    expect(body.mode).toBeUndefined();
    expect(body.printer).toBe("p1");
    expect(await screen.findByText("Sent 2 labels to Label Printer")).toBeInTheDocument();
    // both rows are annotated ok in the grid (regression guard for successful-row annotations)
    expect(await screen.findAllByText("ok")).toHaveLength(2);
  });

  it("maps a print failure to the right source row via copy expansion", async () => {
    fetchMock = stubFetch(() => json({ total: 4, sent: 3, failed: [{ index: 3, error: "boom" }], jobs: 1 }));
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText(/printer/i), { target: { value: "p1" } });
    fireEvent.click(await screen.findByRole("button", { name: /^print$/i }));
    // index 3 with copies=2 maps to source row 1 (sku=2), NOT row 0/row 3: assert it lands on the sku=2 row.
    const failedRow = (await screen.findByText(/failed: boom/i)).closest('[role="row"]') as HTMLElement;
    expect(within(failedRow).getByDisplayValue("2")).toBeInTheDocument();
    expect(within(failedRow).queryByDisplayValue("1")).not.toBeInTheDocument();
  });

  it("maps a 422 BatchInvalid failure to its row and shows a form error", async () => {
    fetchMock = stubFetch(() =>
      json(
        { error: { code: "BatchInvalid", message: "row invalid", details: { failures: [{ index: 0, code: "UnsupportedLayoutItem", message: "missing sku", details: { reason: "missing_field", field: "sku" } }] } } },
        422,
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await loadTemplateAndCsv();
    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());
    fireEvent.click(download);
    // index 0 maps to the first CSV row (sku=1): the annotation lands on that row.
    const failedRow = (await screen.findByText(/failed: missing sku/i)).closest('[role="row"]') as HTMLElement;
    expect(within(failedRow).getByDisplayValue("1")).toBeInTheDocument();
    // a form-level error in the sticky action bar (not the row annotation, which reads "failed: missing sku").
    expect(screen.getByText("missing sku", { selector: "span" })).toBeInTheDocument();
  });

  it("blocks a malformed CSV from being submitted", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: 'sku\n"open' } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    expect(await screen.findByText(/parse error/i)).toBeInTheDocument();
    // No grid or Run buttons render, so nothing can be posted.
    expect(screen.queryByRole("button", { name: /download/i })).not.toBeInTheDocument();
    expect(countCalls("/api/render")).toBe(0);
  });

  it("loads a CSV with no template, then shows options + actions once a template is chosen", async () => {
    renderPage();
    await screen.findByRole("option", { name: "Tag" });
    // Load a CSV before any template is selected.
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n2\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    const grid = await screen.findByRole("grid", { name: /label rows/i });
    // Data columns render; no template means no option controls and no Print/Download.
    expect(within(grid).getByDisplayValue("1")).toBeInTheDocument();
    // Choosing a template reveals the action bar; the loaded rows persist.
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "t1" } });
    expect(await screen.findByRole("button", { name: /download/i })).toBeInTheDocument();
    expect(within(grid).getByDisplayValue("1")).toBeInTheDocument();
    expect(within(grid).getByDisplayValue("2")).toBeInTheDocument();
  });

  it("keeps the CSV rows across a template switch", async () => {
    renderPage();
    await loadTemplateAndCsv();
    const grid = await screen.findByRole("grid", { name: /label rows/i });
    expect(within(grid).getByDisplayValue("1")).toBeInTheDocument();
    // Switch back to no template and to t1 again: rows survive (no remount discards them).
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "" } });
    expect(within(grid).getByDisplayValue("1")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "t1" } });
    expect(within(grid).getByDisplayValue("1")).toBeInTheDocument();
    expect(within(grid).getByDisplayValue("2")).toBeInTheDocument();
  });

  it("preserves a row's raw CSV field across a no-template edit then template pick", async () => {
    renderPage();
    await screen.findByRole("option", { name: "Tag" });
    // Load a CSV carrying color while NO template is selected (t1 is not yet known).
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku,color\n1,blue\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    // Edit the sku cell while still template-less: this commits the displayed field map for the row.
    const skuCell = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    fireEvent.change(skuCell, { target: { value: "9" } });
    // Now pick t1 (which declares color) and submit; the original raw color ("blue") must survive the edit.
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "t1" } });
    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    expect(body.labels[0]).toEqual({ data: { sku: "9", color: "blue" } });
  });

  it("omits a select column the CSV does not carry", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    expect(body.labels[0]).toEqual({ data: { sku: "1" } });
  });

  it("renders a parameter as a column in the grid", async () => {
    const detail2 = {
      ...detail,
      id: "t2",
      name: "Tag2",
      params: [
        { name: "sku", type: "string" as const, control: "text" as const },
        { name: "finish", type: "enum" as const, control: "select" as const, values: ["matte"] },
      ],
    };
    fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/templates/t2")) return json(detail2);
      if (url.startsWith("/api/templates")) return json({ templates: [{ id: "t2", name: "Tag2", description: "", unit: "mm", dpi: 300, format: detail2.format }] });
      if (url.startsWith("/api/printers")) return json(printers);
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag2" });
    fireEvent.change(picker, { target: { value: "t2" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);
    expect(screen.getByText("finish")).toBeInTheDocument();
  });

  it("blocks a CSV with more rows than the 500 cap at load", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    const big = "sku\n" + Array.from({ length: 501 }, (_, i) => String(i)).join("\n");
    fireEvent.change(csv, { target: { value: big } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    expect(await screen.findByText(/limit is 500/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /download/i })).not.toBeInTheDocument();
  });

  it("renders a preview for the selected row and keeps actions enabled on preview error", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:preview");
    vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
    // First render/label call succeeds; subsequent calls error to test the "keeps actions enabled" branch.
    let renderCallCount = 0;
    fetchMock = stubFetch(undefined, () => {
      renderCallCount += 1;
      if (renderCallCount === 1) {
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      }
      return new Response(JSON.stringify({ error: { code: "RenderError", message: "bad row" } }), {
        status: 422,
        headers: { "content-type": "application/json" },
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    renderPage();
    await loadTemplateAndCsv();

    // Default selection is the first valid row, so a render/label call fires immediately.
    await waitFor(() => expect(countCalls("/api/render/label")).toBeGreaterThan(0));

    // Select row 2 -> another render fires (which will error per our stub).
    const before = countCalls("/api/render/label");
    fireEvent.click(screen.getByLabelText("preview row 2"));
    await waitFor(() => expect(countCalls("/api/render/label")).toBe(before + 1));

    // Download stays enabled even though the preview endpoint errored.
    expect(screen.getByRole("button", { name: /download/i })).not.toBeDisabled();
  });

  it("imports a CSV with a quoted multiline field, displays the line-count marker, and edits with Shift+Enter", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
    const multilineDetail = {
      ...detail,
      id: "t3",
      name: "Tag3",
      params: [
        { name: "sku", type: "string" as const, control: "text" as const },
        { name: "message", type: "string" as const, control: "textarea" as const },
      ],
    };

    fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/templates/t3")) return json(multilineDetail);
      if (url.startsWith("/api/templates")) return json({ templates: [{ id: "t3", name: "Tag3", description: "", unit: "mm", dpi: 300, format: detail.format }] });
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label")) {
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      }
      if (url.startsWith("/api/render")) {
        return new Response(new Blob(["zip"]), { status: 200, headers: { "content-type": "application/zip" } });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag3" });
    fireEvent.change(picker, { target: { value: "t3" } });

    // Import a CSV whose quoted field holds a newline
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: 'sku,message\n1,"line one\nline two"\n' } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);

    // Confirm the cell shows the textarea with value and the line-count marker
    const textarea = (await screen.findByLabelText("edit message")) as HTMLTextAreaElement;
    expect(textarea.tagName).toBe("TEXTAREA");
    expect(textarea.value).toBe("line one\nline two");
    expect(screen.getByText("+1")).toBeInTheDocument();

    // Edit it
    fireEvent.change(textarea, { target: { value: "first line\nsecond line\nthird line" } });

    // Confirm updated display has '+2' marker
    expect(textarea.value).toBe("first line\nsecond line\nthird line");
    expect(screen.getByText("+2")).toBeInTheDocument();

    // Submit download and confirm submitted payload has the newlines intact
    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    expect(body.labels[0]).toEqual({ data: { sku: "1", message: "first line\nsecond line\nthird line" } });
  });
});

// #209: a `datetime` parameter is optional (blank means the server's render instant) but a value
// that cannot be parsed must stop the run before it is submitted.
describe("CSV Import screen: datetime parameters", () => {
  let dtControl: "datetime" | "date" = "datetime";
  const dtDetail = {
    ...detail,
    params: [
      { name: "sku", type: "string" as const, control: "text" as const },
      { name: "printed_on", type: "datetime" as const, control: "datetime" as const, description: "Print date" },
    ],
  };
  const dtList = { templates: [{ ...list.templates[0], format: dtDetail.format }] };

  function stubDatetimeFetch() {
    return vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      const tDetail = {
        ...dtDetail,
        params: [
          { name: "sku", type: "string" as const, control: "text" as const },
          { name: "printed_on", type: "datetime" as const, control: dtControl, description: "Print date" },
        ],
      };
      if (url.startsWith("/api/templates/t1")) return json(tDetail);
      if (url.startsWith("/api/templates")) return json(dtList);
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label"))
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url === "/api/render")
        return new Response(new Blob(["zip"]), { status: 200, headers: { "content-type": "application/zip" } });
      if (url === "/api/print") return json(summary);
      throw new Error(`unexpected fetch: ${url}`);
    });
  }

  beforeEach(() => {
    dtControl = "datetime";
    vi.unstubAllGlobals();
    fetchMock = stubDatetimeFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  async function loadCsv(printedOn: string) {
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: `sku,printed_on\n1,${printedOn}\n` } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);
  }

  it("leaves a blank datetime cell valid and submits it without a value", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderPage();
    await loadCsv("");

    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());

    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    expect(body.labels[0].data.printed_on).toBeUndefined();
  });

  it("accepts a well-formed datetime cell", async () => {
    renderPage();
    await loadCsv("2026-08-19");
    expect(await screen.findByRole("button", { name: /download/i })).not.toBeDisabled();
  });

  it("flags an unparseable datetime cell and blocks the run", async () => {
    renderPage();
    await loadCsv("not a date");

    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).toBeDisabled());
    fireEvent.click(download);
    expect(countCalls("/api/render")).toBe(0);
  });

  it("flags a datetime cell that is well-shaped but not a real date", async () => {
    renderPage();
    await loadCsv("2026-02-30");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /download/i })).toBeDisabled(),
    );
  });

  it("flags an unparseable cell on a date control and blocks the run", async () => {
    dtControl = "date";
    renderPage();
    await loadCsv("not a date");

    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).toBeDisabled());
    fireEvent.click(download);
    expect(countCalls("/api/render")).toBe(0);
  });

  it("skips list parameters when building grid columns and does not break import", async () => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    fetchMock = vi.fn(async (input: RequestInfo | URL, _init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/templates/t1")) {
        return json({
          ...detail,
          params: [
            { name: "sku", type: "string", control: "text" },
            { name: "tags", type: "list", control: "list" },
          ],
        });
      }
      if (url.startsWith("/api/templates")) return json(list);
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label")) return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url.startsWith("/api/render")) {
        return new Response(new Blob(["zip"]), { status: 200, headers: { "content-type": "application/zip" } });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();

    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    // Include a list column in the CSV: without the Import.tsx filter it would render as a `--` column
    // and without the pruneDataForSubmit guard it would be sent as `tags: "red;blue"` and get 400.
    fireEvent.change(csv, { target: { value: "sku,tags\n123,red;blue\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);

    // csvFields contains "tags" but displayedFields must filter it out
    expect(await screen.findByDisplayValue("123")).toBeInTheDocument();
    expect(screen.queryByText("tags")).toBeNull();
    // The grid must not show an inert column for the list field
    expect(screen.queryByText("red;blue")).toBeNull();
    const download = await screen.findByRole("button", { name: /download/i });
    expect(download).toBeEnabled();
    fireEvent.click(download);
    await waitFor(() => expect(fetchMock.mock.calls.some(([u]) => String(u) === "/api/render")).toBe(true));
    const batchCall = fetchMock.mock.calls.find(([u]) => String(u) === "/api/render")!;
    const body = JSON.parse((batchCall[1] as RequestInit).body as string);
    // A list column carried as a CSV string must not reach the batch body (pruneDataForSubmit guard).
    expect(body.labels).toHaveLength(1);
    expect(body.labels[0].data.sku).toBe("123");
    expect(body.labels[0].data.tags).toBeUndefined();
  });
});

describe("issue-386: sheet preview", () => {
  const sheetDetail = {
    id: "sheet-tpl",
    name: "Sheet",
    description: "",
    unit: "mm",
    dpi: 300,
    format: {
      type: "sheet",
      width: 210,
      height: 297,
      positions: Array.from({ length: 30 }, () => ({ x: 0, y: 0 })),
    },
    params: [{ name: "sku", type: "string", control: "text" }],
  };

  const importTemplates = [
    { id: "sheet-tpl", name: "Sheet", description: "", unit: "mm", dpi: 300, format: sheetDetail.format },
    { id: "t1", name: "Tag", description: "", unit: "mm", dpi: 300, format: detail.format },
  ];

  type BatchPayload = {
    mode?: string; // asserted absent: /api/render has no mode
    template?: string;
    start_slot?: number;
    labels?: Array<{ data: Record<string, unknown> }>;
  };

  type StubOpts = {
    batch?: (body: Record<string, unknown>) => Response;
    renderLabel?: () => Response;
    templateDetails?: Record<string, unknown>;
  };

  function stubSheetFetch(opts?: StubOpts) {
    return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();


      if (url === "/api/templates") {
        return json({ templates: importTemplates });
      }

      if (url.startsWith("/api/templates/")) {
        const id = url.slice("/api/templates/".length);
        if (opts?.templateDetails && opts.templateDetails[id]) return json(opts.templateDetails[id]);
        if (id === "sheet-tpl") return json(sheetDetail);
        if (id === "t1") return json(detail);
      }

      if (url.startsWith("/api/printers")) return json(printers);

      if (url.startsWith("/api/render/label") && method === "POST") {
        if (opts?.renderLabel) return opts.renderLabel();
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      }

      if (url.startsWith("/api/render") && method === "POST") {
        const body = (init?.body ? JSON.parse(init.body as string) : {}) as Record<string, unknown>;
        if (opts?.batch) return opts.batch(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      }

      throw new Error(`unexpected fetch: ${url} ${method}`);
    });
  }

  async function loadSheetAndCsv(csvText = "sku\n1\n2\n", opts?: StubOpts) {
    fetchMock = stubSheetFetch(opts);
    vi.stubGlobal("fetch", fetchMock);

    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Sheet" });
    fireEvent.change(picker, { target: { value: "sheet-tpl" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: csvText } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByRole("grid", { name: /label rows/i });
  }

  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:preview");
    vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("5.1 Sheet template with 2 valid rows, copies 3 and start slot 1: one POST /api/render preview request with 6 labels in row order and start_slot: 1; activating Download then sends a body whose labels and start_slot deep-equal the preview's", async () => {
    let capturedBatchBodies: BatchPayload[] = [];
    await loadSheetAndCsv("sku\n1\n2\n", {
      batch: (body) => {
        capturedBatchBodies.push(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      },
    });

    await waitFor(() => expect(capturedBatchBodies.length).toBe(1));
    capturedBatchBodies = [];

    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "3" } });
    fireEvent.change(screen.getByLabelText(/start slot/i), { target: { value: "1" } });

    await waitFor(() => expect(capturedBatchBodies.length).toBe(1));
    const previewBody = capturedBatchBodies[0];
    expect(previewBody.mode).toBeUndefined();
    expect(previewBody.template).toBe("sheet-tpl");
    expect(previewBody.start_slot).toBe(1);
    expect(previewBody.labels).toEqual([
      { data: { sku: "1" } },
      { data: { sku: "1" } },
      { data: { sku: "1" } },
      { data: { sku: "2" } },
      { data: { sku: "2" } },
      { data: { sku: "2" } },
    ]);
    expect(screen.getByLabelText(/Sheet preview/i).tagName).toBe("OBJECT");

    capturedBatchBodies = [];
    const downloadBtn = screen.getByRole("button", { name: /^download$/i });
    fireEvent.click(downloadBtn);

    await waitFor(() => expect(capturedBatchBodies.length).toBe(1));
    const downloadBody = capturedBatchBodies[0];
    expect(downloadBody.labels).toEqual(previewBody.labels);
    expect(downloadBody.start_slot).toEqual(previewBody.start_slot);
  });

  it("5.2 Sheet template renders no input[name=\"preview-row\"]; a single template renders one radio per row and the existing selected-row preview tests stay green; no template chosen keeps radios", async () => {
    fetchMock = stubSheetFetch();
    vi.stubGlobal("fetch", fetchMock);

    renderPage();

    // 1. With no template chosen: load CSV, grid keeps radios
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n2\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByRole("grid", { name: /label rows/i });
    expect(document.querySelectorAll('input[name="preview-row"]').length).toBe(2);

    // 2. Select single template: renders one radio per row
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    fireEvent.change(picker, { target: { value: "t1" } });
    await waitFor(() => {
      expect(document.querySelectorAll('input[name="preview-row"]').length).toBe(2);
    });

    // 3. Select sheet template: renders no radios
    fireEvent.change(picker, { target: { value: "sheet-tpl" } });
    await waitFor(() => {
      expect(document.querySelectorAll('input[name="preview-row"]').length).toBe(0);
    });
  });

  it("switching template to a sheet template leaves focus on the picker", async () => {
    fetchMock = stubSheetFetch();
    vi.stubGlobal("fetch", fetchMock);

    renderPage();

    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });

    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n2\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByRole("grid", { name: /label rows/i });

    picker.focus();
    expect(document.activeElement).toBe(picker);

    fireEvent.change(picker, { target: { value: "sheet-tpl" } });
    await waitFor(() => {
      expect(document.querySelectorAll('input[name="preview-row"]').length).toBe(0);
    });

    expect(document.activeElement).toBe(picker);
  });

  // A row the grid refuses is one holding a datetime it cannot parse; the CSV is the only way to put
  // such a value into a cell, since the picker itself only produces well-formed ones.
  const datedSheet = {
    ...sheetDetail,
    params: [
      { name: "sku", type: "string", control: "text" },
      { name: "printed_on", type: "datetime", control: "datetime", time: true },
    ],
  };

  it("5.3 Sheet template with one row holding an unparseable datetime: no /api/render request, no <object>, pane reads Fix row N to preview the sheet.; fixing the cell sends one batch request holding every row and the pane embeds the PDF", async () => {
    const capturedBatchBodies: BatchPayload[] = [];
    await loadSheetAndCsv("sku,printed_on\n1,\n2,not a date\n", {
      templateDetails: { "sheet-tpl": datedSheet },
      batch: (body) => {
        capturedBatchBodies.push(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      },
    });

    await waitFor(() => {
      expect(screen.getByText("Fix row 2 to preview the sheet.")).toBeInTheDocument();
    });
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector("object")).toBeNull();
    expect(capturedBatchBodies.length).toBe(0);

    const grid = screen.getByRole("grid", { name: /label rows/i });
    const pickers = within(grid).getAllByLabelText("edit printed_on");
    fireEvent.change(pickers[1], { target: { value: "2026-08-19T10:00" } });
    await waitFor(() => {
      expect(capturedBatchBodies.length).toBe(1);
    });
    expect(capturedBatchBodies[0].labels).toEqual([
      { data: { sku: "1" } },
      { data: { sku: "2", printed_on: "2026-08-19T10:00" } },
    ]);
    await waitFor(() => {
      expect(document.querySelector("object")).not.toBeNull();
    });
  });

  it("5.4 Sheet template with a 5-row grid whose rows 2 and 5 are invalid: pane reads exactly Fix rows 2, 5 to preview the sheet. and the text does not begin with Preview failed", async () => {
    await loadSheetAndCsv("sku,printed_on\n1,\n2,nope\n3,\n4,\n5,2026-02-30\n", {
      templateDetails: { "sheet-tpl": datedSheet },
    });

    await waitFor(() => {
      expect(screen.getByText("Fix rows 2, 5 to preview the sheet.")).toBeInTheDocument();
    });
    expect(screen.queryByText(/Preview failed/)).toBeNull();
  });

  it("5.5 Sheet template with 2 rows and copies set to 300: no /api/render request and the pane reads Over the 500-label limit; reduce the batch to preview the sheet.", async () => {
    let capturedBatchBodies: BatchPayload[] = [];
    await loadSheetAndCsv("sku\n1\n2\n", {
      batch: (body) => {
        capturedBatchBodies.push(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      },
    });

    await waitFor(() => expect(capturedBatchBodies.length).toBeGreaterThan(0));
    capturedBatchBodies = [];

    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "300" } });

    await waitFor(() => {
      expect(
        screen.getByText("Over the 500-label limit; reduce the batch to preview the sheet."),
      ).toBeInTheDocument();
    });
    await new Promise((r) => setTimeout(r, 400));
    expect(capturedBatchBodies.length).toBe(0);
  });

  it("5.7 Single template: select row 2, clear its value, and stub /api/render/label to return a non-2xx envelope; row 2 stays the row requested, the pane shows Preview failed: with the service's message, and Download stays enabled", async () => {
    let shouldFailRender = false;

    fetchMock = stubSheetFetch({
      templateDetails: {
        t1: {
          ...detail,
          params: [{ name: "sku", type: "string", control: "text" }],
        },
      },
      renderLabel: () => {
        if (shouldFailRender) {
          return new Response(JSON.stringify({ error: { message: "cannot render empty label" } }), {
            status: 422,
            headers: { "content-type": "application/json" },
          });
        }
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n2\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByRole("grid", { name: /label rows/i });

    fireEvent.click(screen.getByLabelText("preview row 2"));

    shouldFailRender = true;
    const grid = screen.getByRole("grid", { name: /label rows/i });
    const textboxes = within(grid).getAllByRole("textbox", { name: /edit sku/i });
    fireEvent.change(textboxes[1], { target: { value: "" } });

    await waitFor(() => {
      expect(screen.getByText("Preview failed: cannot render empty label")).toBeInTheDocument();
    });

    const radio2 = screen.getByLabelText("preview row 2") as HTMLInputElement;
    expect(radio2.checked).toBe(true);
    expect(screen.getByRole("button", { name: /^download$/i })).toBeEnabled();
  });

  it("5.8 Sheet template: a settled edit to a cell, then to copies, then to start slot, each sends one batch request carrying the new labels count or start_slot; a re-render with the batch unchanged sends none", async () => {
    const batchCalls: BatchPayload[] = [];
    await loadSheetAndCsv("sku\n1\n2\n", {
      batch: (body) => {
        batchCalls.push(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      },
    });

    await waitFor(() => expect(batchCalls.length).toBe(1));
    expect(batchCalls[0]?.labels?.[0]?.data?.sku).toBe("1");

    // 1. Settled edit to a cell
    const grid = screen.getByRole("grid", { name: /label rows/i });
    const textboxes = within(grid).getAllByRole("textbox", { name: /edit sku/i });
    fireEvent.change(textboxes[0], { target: { value: "99" } });

    await waitFor(() => expect(batchCalls.length).toBe(2));
    expect(batchCalls[1]?.labels?.[0]?.data?.sku).toBe("99");

    // 2. Edit copies
    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "2" } });
    await waitFor(() => expect(batchCalls.length).toBe(3));
    expect(batchCalls[2]?.labels?.length).toBe(4);

    // 3. Edit start slot
    fireEvent.change(screen.getByLabelText(/start slot/i), { target: { value: "3" } });
    await waitFor(() => expect(batchCalls.length).toBe(4));
    expect(batchCalls[3].start_slot).toBe(3);

    // 4. Re-render with batch unchanged (e.g. change printer)
    fireEvent.change(screen.getByLabelText(/^printer$/i), { target: { value: "p1" } });
    await new Promise((r) => setTimeout(r, 400));
    expect(batchCalls.length).toBe(4);
  });
});

// #413: every screen works from the template detail's published `params`; the per-label inputs
// endpoint is gone, and a checkbox always sends its value.
describe("issue-413: Import reads the published parameter list", () => {
  const paramsDetail = {
    id: "t1",
    name: "Tag",
    description: "",
    categories: [],
    unit: "mm",
    dpi: 300,
    format: { type: "single", width: 80, height: 24 },
    params: [
      { name: "title", type: "string", control: "text" },
      { name: "subtitle", type: "string", control: "text" },
      { name: "code", type: "string", control: "text" },
      { name: "tags", type: "list", control: "list" },
      { name: "flag", type: "boolean", control: "checkbox" },
      { name: "on", type: "boolean", control: "checkbox", default: true },
    ],
    variables: [],
  };

  function stubParams() {
    return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/templates/t1")) return json(paramsDetail);
      if (url.startsWith("/api/templates")) return json(list);
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label")) return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url.startsWith("/api/render")) {
        void init;
        return new Response(new Blob(["zip"]), { status: 200, headers: { "content-type": "application/zip" } });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
  }

  async function loadCsv(text: string) {
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: text } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);
  }

  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
    fetchMock = stubParams();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("A18: draws columns from detail.params and never requests /inputs", async () => {
    renderPage();
    await loadCsv("subtitle,title\nsub,tit\n");
    const headers = screen.getAllByRole("columnheader").map((h) => h.textContent);
    const fieldHeaders = headers.filter((h) => paramsDetail.params.some((p) => p.name === h));
    expect(fieldHeaders).toEqual(["subtitle", "title", "code", "flag", "on"]);
    const download = screen.getByRole("button", { name: /^download$/i });
    await waitFor(() => expect(download).toBeEnabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    expect(fetchMock.mock.calls.filter(([u]) => String(u).includes("/inputs"))).toEqual([]);
  });

  it("A20: an untouched row submits each checkbox's start state, and a held value unchanged", async () => {
    renderPage();
    // `flag` is held blank in row 1 and "yes" in row 2; `on` has no CSV column at all.
    await loadCsv("title,flag\nfirst,\nsecond,yes\n");
    const first = { title: "first", flag: false, on: true };
    // A blank or absent cell shows its parameter's start state; "yes" shows unchecked beside its text.
    const boxes = (name: string) => (screen.getAllByLabelText(`edit ${name}`) as HTMLInputElement[]).map((b) => b.checked);
    expect(boxes("flag")).toEqual([false, false]);
    expect(boxes("on")).toEqual([true, true]);
    expect(screen.getByText("yes")).toBeInTheDocument();
    // Row 1 is previewed by default: the preview body is what the submit path sends for it.
    await waitFor(() => expect(countCalls("/api/render/label")).toBeGreaterThan(0));
    const previewBody = JSON.parse((lastCall("/api/render/label")![1] as RequestInit).body as string);
    expect(previewBody.data).toEqual(first);

    const download = screen.getByRole("button", { name: /^download$/i });
    await waitFor(() => expect(download).toBeEnabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    expect(body.labels).toEqual([
      { data: first },
      { data: { title: "second", flag: "yes", on: true } },
    ]);
  });

  it("A20: a checkbox cell toggles between two states and is never indeterminate", async () => {
    renderPage();
    await loadCsv("title,flag\nfirst,\n");
    const box = () => screen.getByLabelText("edit flag") as HTMLInputElement;
    const seen: Array<[boolean, boolean]> = [[box().checked, box().indeterminate]];
    for (let i = 0; i < 3; i += 1) {
      fireEvent.click(box());
      await waitFor(() => expect(box().checked).toBe(i % 2 === 0));
      seen.push([box().checked, box().indeterminate]);
    }
    expect(seen).toEqual([
      [false, false],
      [true, false],
      [false, false],
      [true, false],
    ]);
  });
});
