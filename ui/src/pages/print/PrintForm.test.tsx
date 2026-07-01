import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { PrintForm } from "./PrintForm";
import type { TemplateDetail } from "../../api/types";

const tape: TemplateDetail = {
  id: "t1",
  name: "Tag",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  layout: [{ type: "text", name: "message" }],
};

const sheet: TemplateDetail = {
  id: "s1",
  name: "Sheet",
  description: "",
  unit: "mm",
  dpi: 300,
  format: {
    type: "sheet",
    paper_width: 210,
    paper_height: 297,
    label_width: 60,
    label_height: 30,
    positions: [
      [0, 0],
      [60, 0],
      [120, 0],
    ],
  },
  layout: [{ type: "text", name: "message" }],
};

const printers = [{ id: "p1", name: "Label Printer", kind: "cups", config: null, enabled: true }];
const summary = { total: 1, succeeded: 1, failed: [], jobs: 1 };

function stubFetch(printersList: unknown[] = printers) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    void init;
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/printers")) {
      return new Response(JSON.stringify(printersList), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.startsWith("/api/render/label")) {
      return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
    }
    if (url.startsWith("/api/print")) {
      return new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.startsWith("/api/batch")) {
      return new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } });
    }
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderForm(detail: TemplateDetail) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <PrintForm detail={detail} />
      </ToastProvider>
    </QueryClientProvider>,
  );
  return qc;
}

let fetchMock: ReturnType<typeof stubFetch>;
// `/api/print` is a prefix of `/api/printers`; match it exactly so printer fetches don't count.
const matches = (u: unknown, path: string) =>
  path === "/api/print" ? String(u) === "/api/print" : String(u).startsWith(path);
const lastCall = (path: string) => [...fetchMock.mock.calls].reverse().find(([u]) => matches(u, path));
const countCalls = (path: string) => fetchMock.mock.calls.filter(([u]) => matches(u, path)).length;

async function fillAndSelectPrinter() {
  const message = (await screen.findByLabelText("message")) as HTMLInputElement;
  fireEvent.change(message, { target: { value: "hello" } });
  fireEvent.change(await screen.findByLabelText("printer"), { target: { value: "p1" } });
}

describe("PrintForm copies", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("routes a tape Print to /api/print with the chosen copies", async () => {
    renderForm(tape);
    await fillAndSelectPrinter();

    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "3" } });

    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);

    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(body.copies).toBe(3);
    expect(body.printer).toBe("p1");
    expect(body.fields).toEqual({ message: "hello" });
    expect(countCalls("/api/batch")).toBe(0);
  });

  it("routes a sheet Print to /api/batch with the label repeated `copies` times", async () => {
    renderForm(sheet);
    await fillAndSelectPrinter();

    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "2" } });

    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);

    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.mode).toBe("print");
    expect(body.labels.length).toBe(2);
    expect(countCalls("/api/print")).toBe(0);
  });

  it("clamps the copies stepper to [1, 100]", async () => {
    renderForm(tape);
    const copies = (await screen.findByLabelText("copies")) as HTMLInputElement;

    fireEvent.change(copies, { target: { value: "999" } });
    expect(copies.value).toBe("100");

    fireEvent.change(copies, { target: { value: "0" } });
    expect(copies.value).toBe("1");
  });
});

describe("PrintForm printer preselect", () => {
  const p = (id: string, enabled: boolean, is_default = false) => ({
    id,
    name: id,
    kind: "cups",
    config: null,
    enabled,
    is_default,
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  const renderWith = (printersList: unknown[]) => {
    fetchMock = stubFetch(printersList);
    vi.stubGlobal("fetch", fetchMock);
    return renderForm(tape);
  };

  const printerSelect = async () => (await screen.findByLabelText("printer")) as HTMLSelectElement;

  it("preselects the enabled default printer over other enabled printers", async () => {
    renderWith([p("a", true), p("b", true, true)]);
    const select = await printerSelect();
    await waitFor(() => expect(select.value).toBe("b"));
  });

  it("preselects the sole enabled printer when there is no default", async () => {
    renderWith([p("only", true), p("off", false)]);
    const select = await printerSelect();
    await waitFor(() => expect(select.value).toBe("only"));
  });

  it("selects none when multiple enabled printers have no default", async () => {
    renderWith([p("a", true), p("b", true)]);
    const select = await printerSelect();
    // Let the preselect effect run; it must leave the selection empty.
    await waitFor(() => expect(countCalls("/api/printers")).toBeGreaterThan(0));
    expect(select.value).toBe("");
    expect(screen.getByRole("button", { name: /^print$/i })).toBeDisabled();
  });

  it("does not clobber an explicit None on a printers refetch (one-shot guard)", async () => {
    const qc = renderWith([p("only", true)]);
    const select = await printerSelect();
    await waitFor(() => expect(select.value).toBe("only"));

    // User explicitly clears to none.
    fireEvent.change(select, { target: { value: "" } });
    expect(select.value).toBe("");

    // A printers refetch must not re-run the one-shot preselect.
    await qc.invalidateQueries({ queryKey: ["printers"] });
    await waitFor(() => expect(countCalls("/api/printers")).toBeGreaterThan(1));
    expect(select.value).toBe("");
  });
});
