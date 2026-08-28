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
  inputs: {
    all: [{ name: "message", control: "text", required: true }],
    default: [{ name: "message", control: "text", required: true }],
  },
  variables: [],
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
  inputs: {
    all: [{ name: "message", control: "text", required: true }],
    default: [{ name: "message", control: "text", required: true }],
  },
  variables: [],
};

const printers = [{ id: "p1", name: "Label Printer", kind: "cups", config: null }];
const summary = { total: 1, succeeded: 1, failed: [], jobs: 1 };

function stubFetch(printersList: unknown[] = printers) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    void init;
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/printers")) {
      return new Response(JSON.stringify(printersList), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (url.startsWith("/api/templates/") && url.includes("/inputs")) {
      return new Response(
        JSON.stringify({
          inputs: [[{ name: "message", control: "text", required: true }]],
        }),
        {
          status: 200,
          headers: { "content-type": "application/json" },
        },
      );
    }
    if (url.startsWith("/api/render/label")) {
      return new Response(new Blob(["img"]), {
        status: 200,
        headers: { "content-type": "image/png" },
      });
    }
    if (url.startsWith("/api/print")) {
      return new Response(JSON.stringify(summary), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (url.startsWith("/api/batch")) {
      return new Response(JSON.stringify(summary), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
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

describe("PrintForm phone-first layout", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("puts copies and Print in the sticky actions row, Download in the secondary row", async () => {
    renderForm(tape);
    const print = await screen.findByRole("button", { name: "Print" });
    const stickyRow = print.closest("div.sticky, div[class*='sticky']");
    expect(stickyRow).not.toBeNull();
    expect(stickyRow).toContainElement(screen.getByLabelText("copies"));
    const download = screen.getByRole("button", { name: "Download" });
    expect(stickyRow).not.toContainElement(download);
  });

  it("on mobile, the preview is a collapsed disclosure and only fetches when opened", async () => {
    vi.stubGlobal(
      "matchMedia",
      (q: string) =>
        ({
          matches: false,
          media: q,
          addEventListener: () => {},
          removeEventListener: () => {},
        }) as unknown as MediaQueryList,
    );
    renderForm(tape);
    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hi" } });
    await new Promise((r) => setTimeout(r, 400));
    expect(countCalls("/api/render/label")).toBe(0);
    fireEvent.click(screen.getByText("Preview"));
    await waitFor(() => expect(countCalls("/api/render/label")).toBeGreaterThan(0));
  });
});

describe("PrintForm gating and submission pruning", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("omits a deactivated name and an empty non-text value from the submitted data", async () => {
    const gatedTemplate: TemplateDetail = {
      ...tape,
      inputs: {
        all: [
          { name: "message", control: "text", required: true },
          { name: "tier", control: "select", values: ["standard", "pro"], default: "standard" },
          { name: "pro_code", control: "text" },
          { name: "count", control: "number" },
        ],
        default: [
          { name: "message", control: "text", required: true },
          { name: "tier", control: "select", values: ["standard", "pro"], default: "standard" },
          { name: "count", control: "number" },
        ],
      },
    };

    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      void init;
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/printers")) {
        return new Response(JSON.stringify([{ id: "p1", name: "P1", kind: "cups", is_default: true }]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/templates/") && url.includes("/inputs")) {
        const bodyStr = init?.body as string | undefined;
        const parsed = bodyStr ? JSON.parse(bodyStr) : null;
        const tier = parsed?.labels?.[0]?.data?.tier;
        const inputs =
          tier === "pro"
            ? [
                { name: "message", control: "text", required: true },
                { name: "tier", control: "select", values: ["standard", "pro"], default: "standard" },
                { name: "pro_code", control: "text" },
              ]
            : [
                { name: "message", control: "text", required: true },
                { name: "tier", control: "select", values: ["standard", "pro"], default: "standard" },
              ];
        return new Response(
          JSON.stringify({
            inputs: [inputs],
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      }
      if (url.startsWith("/api/print")) {
        return new Response(JSON.stringify(summary), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response("{}", { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);

    renderForm(gatedTemplate);
    const message = (await screen.findByLabelText("message")) as HTMLInputElement;
    fireEvent.change(message, { target: { value: "hello" } });

    // Switch to pro tier to reveal pro_code
    const tierSelect = (await screen.findByLabelText("tier")) as HTMLSelectElement;
    fireEvent.change(tierSelect, { target: { value: "pro" } });

    const proCode = (await screen.findByLabelText("pro_code")) as HTMLInputElement;
    fireEvent.change(proCode, { target: { value: "SECRET_CODE" } });

    // Switch back to standard tier to deactivate pro_code
    fireEvent.change(tierSelect, { target: { value: "standard" } });
    await waitFor(() => expect(screen.queryByLabelText("pro_code")).toBeNull());

    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);

    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    // count is empty non-text; pro_code has value but is deactivated
    expect(body.fields).toEqual({ message: "hello", tier: "standard" });
  });
});
