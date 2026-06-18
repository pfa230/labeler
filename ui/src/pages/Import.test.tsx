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
  options: { color: ["red", "blue"] },
  layout: [{ type: "text", name: "sku" }],
};
const list = { templates: [{ id: "t1", name: "Tag", description: "", unit: "mm", dpi: 300, format: detail.format, options: detail.options }] };
const printers = [{ id: "p1", name: "Label Printer", kind: "cups", config: null, enabled: true }];
const summary = { total: 2, succeeded: 2, failed: [], jobs: 1 };

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

// Optional `batch` override lets a test return a custom /api/batch response (failures, 422, etc.).
function stubFetch(batch?: (body: Record<string, unknown>) => Response) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/templates/t1")) return json(detail);
    if (url.startsWith("/api/templates")) return json(list);
    if (url.startsWith("/api/printers")) return json(printers);
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
  fireEvent.change(csv, { target: { value: "sku,option.color\n1,red\n2,blue\n" } });
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
    const file = new File(["sku,option.color\n7,blue\n"], "labels.csv", { type: "text/csv" });
    fireEvent.change(fileInput, { target: { files: [file] } });
    expect(await screen.findByText("7")).toBeInTheDocument();
  });

  it("loads a CSV dropped onto the dropzone", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const zone = await screen.findByLabelText(/csv dropzone/i);
    const file = new File(["sku,option.color\n8,red\n"], "labels.csv", { type: "text/csv" });
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
    expect(body.labels[0]).toEqual({ data: { sku: "1" }, option: { color: "red" } });
    // submitBatch read a binary blob and saved it via an object URL.
    await waitFor(() => expect(createUrl).toHaveBeenCalled());
    expect(body.start_slot).toBeUndefined(); // single template: start_slot omitted
  });

  it("includes manual (global) options in the request when the CSV omits the column", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n2\n" } }); // no option.color column
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByText("1");
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    // The manual strip defaults color to its first declared value and applies it to every row.
    expect(body.labels[0]).toEqual({ data: { sku: "1" }, option: { color: "red" } });
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
    expect(screen.queryByRole("button", { name: /download/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /apply color to all rows/i })).not.toBeInTheDocument();
    // Choosing a template reveals option columns + the action bar; the loaded rows persist.
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "t1" } });
    expect(await screen.findByRole("button", { name: /download/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /apply color to all rows/i })).toBeInTheDocument();
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

  it("preserves a row's raw CSV option across a no-template edit then template pick", async () => {
    renderPage();
    await screen.findByRole("option", { name: "Tag" });
    // Load a CSV carrying option.color while NO template is selected (t1 is not yet known).
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku,option.color\n1,blue\n" } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    // Edit the sku cell while still template-less: this commits the displayed option map for the row.
    fireEvent.doubleClick(await screen.findByText("1")); // enter edit mode (react-data-grid default)
    const skuCell = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    fireEvent.change(skuCell, { target: { value: "9" } });
    fireEvent.blur(skuCell);
    // Now pick t1 (which declares color) and submit; the original raw color ("blue") must survive the edit.
    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "t1" } });
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.labels[0]).toEqual({ data: { sku: "9" }, option: { color: "blue" } });
  });

  it("defaults a per-row option to the first allowed value when the CSV omits it", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n" } }); // no option.color column
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByLabelText(/copies/i);
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    // color defaulted to its first allowed value ("red") on the row.
    expect(body.labels[0]).toEqual({ data: { sku: "1" }, option: { color: "red" } });
  });

  it("applies an option to every row only on the Apply-to-all click", async () => {
    renderPage();
    await loadTemplateAndCsv(); // rows: color red, color blue
    await screen.findByText("1");
    // Merely changing the apply selector must NOT mutate any row.
    const selector = screen.getByLabelText(/set all color/i) as HTMLSelectElement;
    fireEvent.change(selector, { target: { value: "blue" } });
    fireEvent.click(await screen.findByRole("button", { name: /^download$/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    let body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.labels.map((l: { option: { color: string } }) => l.option.color)).toEqual(["red", "blue"]);
    // Clicking Apply to all overwrites every row's color.
    fireEvent.click(screen.getByRole("button", { name: /apply color to all rows/i }));
    fireEvent.click(screen.getByRole("button", { name: /^download$/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(2));
    body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.labels.map((l: { option: { color: string } }) => l.option.color)).toEqual(["blue", "blue"]);
  });

  it("renders a single-valued option as a column without an Apply-to-all control", async () => {
    // t2 declares a single-valued option; it must not get an Apply-to-all control.
    const detail2 = { ...detail, id: "t2", name: "Tag2", options: { finish: ["matte"] } };
    fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/templates/t2")) return json(detail2);
      if (url.startsWith("/api/templates")) return json({ templates: [{ id: "t2", name: "Tag2", description: "", unit: "mm", dpi: 300, format: detail2.format, options: detail2.options }] });
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
    // The option column header is present but no Apply-to-all control nor an inline editor for it.
    expect(screen.getByText("option.finish")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /apply finish to all rows/i })).not.toBeInTheDocument();
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
});
