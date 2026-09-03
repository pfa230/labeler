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
  inputs: {
    all: [
      { name: "sku", control: "text" },
      { name: "color", control: "select", values: ["red", "blue"] },
    ],
    default: [
      { name: "sku", control: "text" },
      { name: "color", control: "select", values: ["red", "blue"] },
    ],
  },
};
const list = { templates: [{ id: "t1", name: "Tag", description: "", unit: "mm", dpi: 300, format: detail.format }] };
const printers = [{ id: "p1", name: "Label Printer", kind: "cups", config: null }];
const summary = { total: 2, succeeded: 2, failed: [], jobs: 1 };

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

// Optional `batch` override lets a test return a custom /api/batch response (failures, 422, etc.).
// Optional `renderLabel` override lets a test control the /api/render/label response.
function stubFetch(
  batch?: (body: Record<string, unknown>) => Response,
  renderLabel?: () => Response,
) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.includes("/inputs")) {
      const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
      const labels = parsedBody.labels ?? [{ data: {} }];
      return json({
        inputs: labels.map(() => [
          { name: "sku", control: "text" },
          { name: "color", control: "select", values: ["red", "blue"] },
        ]),
      });
    }
    if (url.startsWith("/api/templates/t1")) return json(detail);
    if (url.startsWith("/api/templates")) return json(list);
    if (url.startsWith("/api/printers")) return json(printers);
    if (url.startsWith("/api/render/label")) {
      if (renderLabel) return renderLabel();
      return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
    }
    if (url.startsWith("/api/batch")) {
      const body = (init?.body ? JSON.parse(init.body as string) : {}) as Record<string, unknown>;
      if (batch) return batch(body);
      // download returns a binary blob; print returns the JSON summary (submitBatch discriminates on content-type).
      if (body.mode === "download") {
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
const lastCall = (path: string) => [...fetchMock.mock.calls].reverse().find(([u]) => String(u).startsWith(path));
const countCalls = (path: string) => fetchMock.mock.calls.filter(([u]) => String(u).startsWith(path)).length;

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
    expect(await screen.findByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
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
    expect(await screen.findByText("7")).toBeInTheDocument();
  });

  it("loads a CSV dropped onto the dropzone", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const zone = await screen.findByLabelText(/csv dropzone/i);
    const file = new File(["sku,color\n8,red\n"], "labels.csv", { type: "text/csv" });
    fireEvent.drop(zone, { dataTransfer: { files: [file] } });
    expect(await screen.findByText("8")).toBeInTheDocument();
  });

  it("posts a download batch for all resolved rows and saves the file", async () => {
    const createUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.template).toBe("t1");
    expect(body.mode).toBe("download");
    expect(body.labels).toHaveLength(2);
    expect(body.labels[0]).toEqual({ data: { sku: "1", color: "red" } });
    // submitBatch read a binary blob and saved it via an object URL.
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
    await screen.findByText("1");
    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
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
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.template).toBe("t1");
    expect(body.mode).toBe("print");
    expect(body.printer).toBe("p1");
    expect(await screen.findByText(/printed 2\/2/i)).toBeInTheDocument();
    // both rows are annotated ok in the grid (regression guard for successful-row annotations)
    expect(await screen.findAllByText("ok")).toHaveLength(2);
  });

  it("maps a print failure to the right source row via copy expansion", async () => {
    fetchMock = stubFetch(() => json({ total: 4, succeeded: 3, failed: [{ index: 3, error: "boom" }], jobs: 1 }));
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText(/printer/i), { target: { value: "p1" } });
    fireEvent.click(await screen.findByRole("button", { name: /^print$/i }));
    // index 3 with copies=2 maps to source row 1 (sku=2), NOT row 0/row 3: assert it lands on the sku=2 row.
    const failedRow = (await screen.findByText(/failed: boom/i)).closest('[role="row"]') as HTMLElement;
    expect(within(failedRow).getByText("2")).toBeInTheDocument();
    expect(within(failedRow).queryByText("1")).not.toBeInTheDocument();
  });

  it("maps a 422 BatchInvalid failure to its row and shows a form error", async () => {
    fetchMock = stubFetch(() =>
      json(
        { error: { code: "BatchInvalid", message: "row invalid", details: { failures: [{ index: 0, code: "MissingField", message: "missing sku" }] } } },
        422,
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    // index 0 maps to the first CSV row (sku=1): the annotation lands on that row.
    const failedRow = (await screen.findByText(/failed: missing sku/i)).closest('[role="row"]') as HTMLElement;
    expect(within(failedRow).getByText("1")).toBeInTheDocument();
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
    expect(countCalls("/api/batch")).toBe(0);
  });

  it("loads a CSV with no template, then shows options + actions once a template is chosen", async () => {
    renderPage();
    await screen.findByRole("option", { name: "Tag" });
    // Load a CSV before any template is selected.
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n2\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    // Data columns render; no template means no option controls and no Print/Download.
    expect(await screen.findByText("1")).toBeInTheDocument();
    // Choosing a template reveals the action bar; the loaded rows persist.
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "t1" } });
    expect(await screen.findByRole("button", { name: /download/i })).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("keeps the CSV rows across a template switch", async () => {
    renderPage();
    await loadTemplateAndCsv();
    expect(await screen.findByText("1")).toBeInTheDocument();
    // Switch back to no template and to t1 again: rows survive (no remount discards them).
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "" } });
    expect(screen.getByText("1")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "t1" } });
    expect(await screen.findByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("preserves a row's raw CSV field across a no-template edit then template pick", async () => {
    renderPage();
    await screen.findByRole("option", { name: "Tag" });
    // Load a CSV carrying color while NO template is selected (t1 is not yet known).
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku,color\n1,blue\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    // Edit the sku cell while still template-less: this commits the displayed field map for the row.
    fireEvent.doubleClick(await screen.findByText("1")); // enter edit mode (react-data-grid default)
    const skuCell = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    fireEvent.change(skuCell, { target: { value: "9" } });
    fireEvent.blur(skuCell);
    // Now pick t1 (which declares color) and submit; the original raw color ("blue") must survive the edit.
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "t1" } });
    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).not.toBeDisabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.labels[0]).toEqual({ data: { sku: "9", color: "blue" } });
  });

  it("defaults a per-row select input when initialized from template defaults", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.labels[0]).toEqual({ data: { sku: "1" } });
  });

  it("renders an input as a column in the grid", async () => {
    const detail2 = {
      ...detail,
      id: "t2",
      name: "Tag2",
      inputs: {
        all: [
          { name: "sku", control: "text" as const },
          { name: "finish", control: "select" as const, values: ["matte"] },
        ],
        default: [
          { name: "sku", control: "text" as const },
          { name: "finish", control: "select" as const, values: ["matte"] },
        ],
      },
    };
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/inputs")) {
        const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
        const labels = parsedBody.labels ?? [{ data: {} }];
        return json({
          inputs: labels.map(() => [
            { name: "sku", control: "text" },
            { name: "finish", control: "select", values: ["matte"] },
          ]),
        });
      }
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
      inputs: {
        all: [
          { name: "sku", control: "text" as const },
          { name: "message", control: "textarea" as const },
        ],
        default: [
          { name: "sku", control: "text" as const },
          { name: "message", control: "textarea" as const },
        ],
      },
    };

    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/inputs")) {
        const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
        const labels = parsedBody.labels ?? [{ data: {} }];
        return json({
          inputs: labels.map(() => [
            { name: "sku", control: "text" },
            { name: "message", control: "textarea" },
          ]),
        });
      }
      if (url.startsWith("/api/templates/t3")) return json(multilineDetail);
      if (url.startsWith("/api/templates")) return json({ templates: [{ id: "t3", name: "Tag3", description: "", unit: "mm", dpi: 300, format: detail.format }] });
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label")) {
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      }
      if (url.startsWith("/api/batch")) {
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

    // Confirm the cell shows the first line and the line-count marker
    expect(await screen.findByText("line one")).toBeInTheDocument();
    expect(screen.getByText("+1")).toBeInTheDocument();

    // Edit it with Shift+Enter
    fireEvent.doubleClick(screen.getByText("line one"));
    const textarea = (await screen.findByLabelText("edit message")) as HTMLTextAreaElement;
    expect(textarea.tagName).toBe("TEXTAREA");
    fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true });
    fireEvent.change(textarea, { target: { value: "first line\nsecond line\nthird line" } });
    fireEvent.blur(textarea);

    await waitFor(() => expect(screen.queryByLabelText("edit message")).toBeNull());
    // Confirm updated display has '+2' marker
    expect(await screen.findByText("first line")).toBeInTheDocument();
    expect(screen.getByText("+2")).toBeInTheDocument();

    // Submit download and confirm submitted payload has the newlines intact
    const download = await screen.findByRole("button", { name: /download/i });
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.labels[0]).toEqual({ data: { sku: "1", message: "first line\nsecond line\nthird line" } });
  });
});

// #209: a `datetime` parameter is optional (blank means the server's render instant) but a value
// that cannot be parsed must stop the run before it is submitted.
describe("CSV Import screen: datetime parameters", () => {
  let dtControl: "datetime" | "date" = "datetime";
  let dtRequired = false;
  const dtDetail = {
    ...detail,
    inputs: {
      all: [
        { name: "sku", control: "text" as const },
        { name: "printed_on", control: "datetime" as const, description: "Print date" },
      ],
      default: [
        { name: "sku", control: "text" as const },
        { name: "printed_on", control: "datetime" as const, description: "Print date" },
      ],
    },
    layout: [{ type: "text", value: "{sku} {printed_on.short_date}" }],
  };
  const dtList = { templates: [{ ...list.templates[0], format: dtDetail.format }] };

  function stubDatetimeFetch() {
    return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/inputs")) {
        const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
        const labels = parsedBody.labels ?? [{ data: {} }];
        return json({
          inputs: labels.map(() => [
            { name: "sku", control: "text" },
            { name: "printed_on", control: dtControl, required: dtRequired, description: "Print date" },
          ]),
        });
      }
      const tDetail = {
        ...dtDetail,
        inputs: {
          all: [
            { name: "sku", control: "text" as const },
            { name: "printed_on", control: dtControl, required: dtRequired, description: "Print date" },
          ],
          default: [
            { name: "sku", control: "text" as const },
            { name: "printed_on", control: dtControl, required: dtRequired, description: "Print date" },
          ],
        },
      };
      if (url.startsWith("/api/templates/t1")) return json(tDetail);
      if (url.startsWith("/api/templates")) return json(dtList);
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label"))
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url.startsWith("/api/batch")) {
        const body = (init?.body ? JSON.parse(init.body as string) : {}) as Record<string, unknown>;
        if (body.mode === "download")
          return new Response(new Blob(["zip"]), { status: 200, headers: { "content-type": "application/zip" } });
        return json(summary);
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
  }

  beforeEach(() => {
    dtControl = "datetime";
    dtRequired = false;
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
    expect(download).not.toBeDisabled();

    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
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
    expect(countCalls("/api/batch")).toBe(0);
  });

  it("flags a datetime cell that is well-shaped but not a real date", async () => {
    renderPage();
    await loadCsv("2026-02-30");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /download/i })).toBeDisabled(),
    );
  });

  it("flags a blank datetime cell when required and blocks the run", async () => {
    dtRequired = true;
    renderPage();
    await loadCsv("");

    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).toBeDisabled());
  });

  // A `datetime` parameter declaring `time: false` is reported as the `date` control, which the
  // grid must validate exactly as it validates `datetime`.
  it("flags an unparseable cell on a date control and blocks the run", async () => {
    dtControl = "date";
    renderPage();
    await loadCsv("not a date");

    const download = await screen.findByRole("button", { name: /download/i });
    await waitFor(() => expect(download).toBeDisabled());
    fireEvent.click(download);
    expect(countCalls("/api/batch")).toBe(0);
  });

  it("surfaces default_error.message for an empty cell whose input carries a broken default", async () => {
    // Override stub to return an input with default_error
    fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/inputs")) {
        return json({
          inputs: [
            [
              {
                name: "sku",
                control: "text",
                required: true,
                default_error: {
                  reason: "param_default_unresolvable",
                  message: "vars.missing not found",
                  token: "vars.missing",
                },
              },
            ],
          ],
        });
      }
      if (url.startsWith("/api/templates/t1")) {
        return json({
          ...detail,
          inputs: {
            all: [{ name: "sku", control: "text", required: true, default_error: { reason: "param_default_unresolvable", message: "vars.missing not found", token: "vars.missing" } }],
            default: [{ name: "sku", control: "text", required: true, default_error: { reason: "param_default_unresolvable", message: "vars.missing not found", token: "vars.missing" } }],
          },
        });
      }
      if (url.startsWith("/api/templates")) return json(list);
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label")) return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url.startsWith("/api/batch")) return new Response(new Blob(["zip"]), { status: 200, headers: { "content-type": "application/zip" } });
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    // Load a CSV where sku is empty, so the required + default_error path is exercised
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku,other\n,foo\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);
    // The grid validation should contain the default_error message, not generic "required"
    expect(await screen.findByText(/vars\.missing/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /download/i })).toBeDisabled();
  });

  it("skips list inputs when building grid columns and does not break import", async () => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    fetchMock = vi.fn(async (input: RequestInfo | URL, _init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/inputs")) {
        return json({
          inputs: [
            [
              { name: "sku", control: "text" },
              { name: "tags", control: "list", required: true },
            ],
          ],
        });
      }
      if (url.startsWith("/api/templates/t1")) {
        return json({
          ...detail,
          inputs: {
            all: [
              { name: "sku", control: "text" },
              { name: "tags", control: "list", required: true },
            ],
            default: [
              { name: "sku", control: "text" },
              { name: "tags", control: "list", required: true },
            ],
          },
        });
      }
      if (url.startsWith("/api/templates")) return json(list);
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label")) return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url.startsWith("/api/batch")) {
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
    expect(await screen.findByText("123")).toBeInTheDocument();
    expect(screen.queryByText("tags")).toBeNull();
    // The grid must not show an inert column for the list field
    expect(screen.queryByText("red;blue")).toBeNull();
    const download = await screen.findByRole("button", { name: /download/i });
    expect(download).toBeEnabled();
    fireEvent.click(download);
    await waitFor(() => expect(fetchMock.mock.calls.some(([u]) => String(u).includes("/api/batch"))).toBe(true));
    const batchCall = fetchMock.mock.calls.find(([u]) => String(u).includes("/api/batch"))!;
    const body = JSON.parse((batchCall[1] as RequestInit).body as string);
    // A list column carried as a CSV string must not reach the batch body (pruneDataForSubmit guard).
    expect(body.labels).toHaveLength(1);
    expect(body.labels[0].data.sku).toBe("123");
    expect(body.labels[0].data.tags).toBeUndefined();
  });

  it("does not require a value for a required list input when the CSV has no column for it", async () => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    fetchMock = vi.fn(async (input: RequestInfo | URL, _init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/inputs")) {
        return json({
          inputs: [
            [
              { name: "sku", control: "text" },
              { name: "tags", control: "list", required: true },
            ],
          ],
        });
      }
      if (url.startsWith("/api/templates/t1")) {
        return json({
          ...detail,
          inputs: {
            all: [
              { name: "sku", control: "text" },
              { name: "tags", control: "list", required: true },
            ],
            default: [
              { name: "sku", control: "text" },
              { name: "tags", control: "list", required: true },
            ],
          },
        });
      }
      if (url.startsWith("/api/templates")) return json(list);
      if (url.startsWith("/api/printers")) return json(printers);
      if (url.startsWith("/api/render/label")) return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url.startsWith("/api/batch")) {
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
    // CSV has no column for the required list — validateRow must skip it (Import.tsx:154)
    fireEvent.change(csv, { target: { value: "sku\n123\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);

    expect(await screen.findByText("123")).toBeInTheDocument();
    const download = await screen.findByRole("button", { name: /download/i });
    // If the `if (input.control === "list") continue` guard regresses, every row is flagged
    // as missing `tags` and Download is disabled — ordinary import is blocked.
    expect(download).toBeEnabled();
    fireEvent.click(download);
    await waitFor(() => expect(fetchMock.mock.calls.some(([u]) => String(u).includes("/api/batch"))).toBe(true));
    const batchCall = fetchMock.mock.calls.find(([u]) => String(u).includes("/api/batch"))!;
    const body = JSON.parse((batchCall[1] as RequestInit).body as string);
    expect(body.labels[0].data.sku).toBe("123");
    expect(body.labels[0].data.tags).toBeUndefined();
  });
});
