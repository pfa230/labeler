import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { PrintForm } from "./PrintForm";
import type { Param, TemplateDetail } from "../../api/types";

const tape: TemplateDetail = {
  params: [{ name: "message", type: "string", control: "text" }],
  id: "t1",
  name: "Tag",
  description: "",
  categories: [],
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  variables: [],
};

const sheet: TemplateDetail = {
  params: [{ name: "message", type: "string", control: "text" }],
  id: "s1",
  name: "Sheet",
  description: "",
  categories: [],
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
  variables: [],
};

const printers = [{ id: "p1", name: "Label Printer", uri: "ipp://p1/q", insecure: false }];
const summary = { total: 1, sent: 1, failed: [], jobs: 1 };

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
const probeOk = (chain: boolean) => ({
  status: "ok",
  capabilities: { color: "bilevel", accepts_png: true, chain_and_continue: chain },
});

// `probes` answers each stored printer's probe by id; a printer it does not name probes without
// chain support.
function stubFetch(
  printersList: unknown[] = printers,
  defaultPrinterId: string | null = null,
  probes: Record<string, () => Promise<Response>> = {},
) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    void init;
    const url = typeof input === "string" ? input : input.toString();
    const probed = url.match(/^\/api\/printers\/([^/]+)\/probe$/);
    if (probed) return probes[probed[1]]?.() ?? json(probeOk(false));
    if (url === "/api/settings") {
      return new Response(
        JSON.stringify({ default_printer_id: { value: defaultPrinterId, is_default: defaultPrinterId === null } }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (url.startsWith("/api/printers")) {
      return new Response(JSON.stringify(printersList), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
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
    if (url === "/api/render") {
      return new Response(new Blob(["PK"]), {
        status: 200,
        headers: { "content-type": "application/zip", "content-disposition": 'attachment; filename="t.zip"' },
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
  path === "/api/print" || path === "/api/render" ? String(u) === path : String(u).startsWith(path);
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

  it("routes a tape Print to /api/print as one label entry carrying `copies`", async () => {
    renderForm(tape);
    await fillAndSelectPrinter();

    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "3" } });

    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);

    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(body).toEqual({ template: tape.id, printer: "p1", labels: [{ data: { message: "hello" }, copies: 3 }] });
    expect(countCalls("/api/render")).toBe(0);
    expect(await screen.findByText("Sent 1 labels to Label Printer")).toBeInTheDocument();
  });

  it("routes a sheet Print to /api/print as one label entry carrying `copies`", async () => {
    renderForm(sheet);
    await fillAndSelectPrinter();

    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "2" } });

    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);

    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(body.labels).toEqual([{ data: { message: "hello" }, copies: 2 }]);
    expect(body.mode).toBeUndefined();
  });

  it("downloads a single template through /api/render as one entry carrying `copies`, in the chosen format", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderForm(tape);
    const message = (await screen.findByLabelText("message")) as HTMLInputElement;
    fireEvent.change(message, { target: { value: "hello" } });
    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText("download format"), { target: { value: "pdf" } });

    fireEvent.click(screen.getByRole("button", { name: /^download$/i }));

    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    expect(body).toEqual({ template: tape.id, labels: [{ data: { message: "hello" }, copies: 2 }], format: "pdf" });
  });

  it("shows a refused label's own error after a Download", async () => {
    fetchMock.mockImplementation(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "/api/render") {
        const failure = { index: 0, code: "UnsupportedLayoutItem", message: "Missing required field 'message'" };
        const body = { error: { code: "BatchInvalid", message: "one or more labels in the batch are invalid", details: { failures: [failure] } } };
        return new Response(JSON.stringify(body), { status: 422, headers: { "content-type": "application/json" } });
      }
      return stubFetch()(input);
    });
    renderForm(tape);
    await screen.findByLabelText("message");
    fireEvent.click(screen.getByRole("button", { name: /^download$/i }));

    expect((await screen.findAllByText("Missing required field 'message'")).length).toBeGreaterThan(0);
    expect(screen.queryByText("one or more labels in the batch are invalid")).toBeNull();
  });

  it("shows every label the printer refused", async () => {
    fetchMock.mockImplementation(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "/api/print") {
        const refused = { total: 3, sent: 1, failed: [{ index: 0, error: "refused" }, { index: 2, error: "unreachable" }], jobs: 3 };
        return new Response(JSON.stringify(refused), { status: 200, headers: { "content-type": "application/json" } });
      }
      return stubFetch()(input);
    });
    renderForm(tape);
    await fillAndSelectPrinter();
    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);

    const toast = await screen.findByText(/^Sent 1 of 3 labels to Label Printer/);
    expect(toast.textContent).toContain("label 1: refused");
    expect(toast.textContent).toContain("label 3: unreachable");
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
  const two = [
    { id: "p1", name: "Label Printer", uri: "ipp://p1/q", insecure: false },
    { id: "p2", name: "Backup Printer", uri: "ipp://p2/q", insecure: false },
  ];
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("preselects the printer named by default_printer_id", async () => {
    fetchMock = stubFetch(two, "p2");
    vi.stubGlobal("fetch", fetchMock);
    renderForm(tape);
    const select = (await screen.findByLabelText("printer")) as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe("p2"));
  });

  // Regression guard: the sole-printer fallback predates the setting.
  it("preselects the only printer when no default is set", async () => {
    fetchMock = stubFetch(printers, null);
    vi.stubGlobal("fetch", fetchMock);
    renderForm(tape);
    const select = (await screen.findByLabelText("printer")) as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe("p1"));
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

  it("leaves undefaulted datetime, boolean, and enum empty on mount, and gates only on a printer", async () => {
    const detailWithTypes: TemplateDetail = {
      params: [
        { name: "printed_on", type: "datetime", control: "datetime", time: true },
        { name: "flag", type: "boolean", control: "checkbox" },
        { name: "choice", type: "enum", control: "select", values: ["one", "two"] },
        { name: "token_field", type: "string", control: "text" },
        { name: "lit_field", type: "string", control: "text", default: "seeded" },
      ],
      id: "types_tpl",
      name: "Types Template",
      description: "",
      categories: [],
      unit: "mm",
      dpi: 300,
      format: { type: "single", width: 80, height: 24 },
      variables: [],
    };

    const fetchMock = stubFetch(printers.concat({ id: "p2", name: "Backup", uri: "ipp://p2/q", insecure: false }));
    vi.stubGlobal("fetch", fetchMock);

    renderForm(detailWithTypes);

    const dtInput = (await screen.findByLabelText("printed_on")) as HTMLInputElement;
    expect(dtInput.value).toBe("");
    expect((screen.getByRole("checkbox", { name: "flag" }) as HTMLInputElement).checked).toBe(false);
    expect((screen.getByLabelText("choice") as HTMLSelectElement).value).toBe("");
    expect((screen.getByLabelText("token_field") as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText("lit_field") as HTMLInputElement).value).toBe("");
    expect(screen.getByText("default: seeded")).toBeInTheDocument();

    // The form does not judge completeness: blank fields leave Download enabled, and Print waits
    // only for a printer.
    const printBtn = screen.getByRole("button", { name: /^print$/i });
    expect(screen.getByRole("button", { name: /^download$/i })).toBeEnabled();
    expect(printBtn).toBeDisabled();
    fireEvent.change(await screen.findByLabelText("printer"), { target: { value: "p1" } });
    await waitFor(() => expect(printBtn).not.toBeDisabled());
  });
});

describe("PrintForm blank means default", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  function stubParams() {
    const mock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/printers")) {
        return new Response(JSON.stringify(printers), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/print")) {
        return new Response(JSON.stringify(summary), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response("{}", { status: 200 });
    });
    fetchMock = mock;
    vi.stubGlobal("fetch", mock);
    return mock;
  }

  const printFields = async () => {
    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    const before = countCalls("/api/print");
    fireEvent.click(print);
    await waitFor(() => expect(countCalls("/api/print")).toBe(before + 1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(body.fields).toBeUndefined();
    return body.labels[0].data as Record<string, unknown>;
  };

  const withParams = (params: Param[]): TemplateDetail => ({ ...tape, id: "def_tpl", params });

  it("starts a defaulted field blank with the default as its hint, and omits it", async () => {
    stubParams();
    renderForm(withParams([
      { name: "message", type: "string", control: "text" },
      { name: "title", type: "string", control: "text", default: "Untitled" },
    ]));
    expect(((await screen.findByLabelText("title")) as HTMLInputElement).value).toBe("");
    expect(screen.getByText("default: Untitled")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(await printFields()).toEqual({});
  });

  it("sends a typed value, and omits it again once cleared", async () => {
    stubParams();
    renderForm(withParams([{ name: "title", type: "string", control: "text", default: "Untitled" }]));
    const title = await screen.findByLabelText("title");
    fireEvent.change(title, { target: { value: "Bolts" } });
    expect(await printFields()).toEqual({ title: "Bolts" });
    fireEvent.change(title, { target: { value: "" } });
    expect(await printFields()).toEqual({});
  });

  it("omits a cleared integer and a cleared undefaulted text", async () => {
    stubParams();
    renderForm(withParams([
      { name: "qty", type: "integer", control: "integer" },
      { name: "note", type: "string", control: "text" },
    ]));
    const qty = await screen.findByLabelText("qty");
    fireEvent.change(qty, { target: { value: "3" } });
    fireEvent.change(qty, { target: { value: "" } });
    fireEvent.change(screen.getByLabelText("note"), { target: { value: "x" } });
    fireEvent.change(screen.getByLabelText("note"), { target: { value: "" } });
    expect(await printFields()).toEqual({});
  });

  it("starts a defaulted list empty with its hint, omits it untouched, and sends [] once emptied", async () => {
    stubParams();
    renderForm(withParams([{ name: "tags", type: "list", control: "list", default: ["CONSUMABLE"] }]));
    expect(await screen.findByText("default: CONSUMABLE")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "tags 1" })).not.toBeInTheDocument();
    expect(await printFields()).toEqual({});
    fireEvent.click(screen.getByRole("button", { name: "add tags" }));
    fireEvent.click(screen.getByRole("button", { name: "remove tags 1" }));
    expect(await printFields()).toEqual({ tags: [] });
  });

  it("omits an untouched undefaulted list", async () => {
    stubParams();
    renderForm(withParams([{ name: "tags", type: "list", control: "list" }]));
    await screen.findByRole("button", { name: "add tags" });
    expect(await printFields()).toEqual({});
  });

  it("drops an image read still in flight when the template changes", async () => {
    stubParams();
    const photo: Param = { name: "photo", type: "string", control: "image" };
    const a: TemplateDetail = { ...withParams([photo]), id: "tpl_a" };
    const b: TemplateDetail = { ...withParams([photo]), id: "tpl_b" };
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const tree = (detail: TemplateDetail) => (
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <PrintForm detail={detail} />
        </ToastProvider>
      </QueryClientProvider>
    );
    const { rerender } = render(tree(a));
    const file = new File(["from-a"], "a.png", { type: "image/png" });
    fireEvent.change(await screen.findByLabelText("photo"), { target: { files: [file] } });
    rerender(tree(b)); // before the FileReader finishes
    await new Promise((r) => setTimeout(r, 50));
    expect(await printFields()).toEqual({});
  });

  it("carries no value across a template change", async () => {
    stubParams();
    const a: TemplateDetail = { ...withParams([{ name: "title", type: "string", control: "text" }]), id: "tpl_a" };
    const b: TemplateDetail = { ...withParams([{ name: "title", type: "string", control: "text", default: "B-title" }]), id: "tpl_b" };
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const tree = (detail: TemplateDetail) => (
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <PrintForm detail={detail} />
        </ToastProvider>
      </QueryClientProvider>
    );
    const { rerender } = render(tree(a));
    fireEvent.change(await screen.findByLabelText("title"), { target: { value: "from A" } });
    rerender(tree(b));
    expect((screen.getByLabelText("title") as HTMLInputElement).value).toBe("");
    expect(screen.getByText("default: B-title")).toBeInTheDocument();
    expect(await printFields()).toEqual({});
  });

  it("submits data with elements in row order after appending twice and typing", async () => {
    const list: Param[] = [{ name: "tags", type: "list", control: "list" }];
    stubParams();
    renderForm(withParams(list));

    const addBtn = await screen.findByRole("button", { name: "add tags" });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 1" }), { target: { value: "A" } });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 2" }), { target: { value: "B" } });

    const data = await printFields();
    expect(data.tags).toEqual(["A", "B"]);
  });

  it("submits element left empty as empty string", async () => {
    const list: Param[] = [{ name: "tags", type: "list", control: "list" }];
    stubParams();
    renderForm(withParams(list));

    const addBtn = await screen.findByRole("button", { name: "add tags" });
    fireEvent.click(addBtn);

    const data = await printFields();
    expect(data.tags).toEqual([""]);
  });

  it("submits data with reordered elements after moving elements", async () => {
    const list: Param[] = [{ name: "tags", type: "list", control: "list" }];
    stubParams();
    renderForm(withParams(list));

    const addBtn = await screen.findByRole("button", { name: "add tags" });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 1" }), { target: { value: "A" } });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 2" }), { target: { value: "B" } });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 3" }), { target: { value: "C" } });

    // Move C one position earlier
    fireEvent.click(screen.getByRole("button", { name: "move tags 3 earlier" }));
    // Move A one position later
    fireEvent.click(screen.getByRole("button", { name: "move tags 1 later" }));

    const data = await printFields();
    expect(data.tags).toEqual(["C", "A", "B"]);
  });

  it("submits data without removed element after removing element", async () => {
    const list: Param[] = [{ name: "tags", type: "list", control: "list" }];
    stubParams();
    renderForm(withParams(list));

    const addBtn = await screen.findByRole("button", { name: "add tags" });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 1" }), { target: { value: "A" } });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 2" }), { target: { value: "B" } });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 3" }), { target: { value: "C" } });

    // Remove B
    fireEvent.click(screen.getByRole("button", { name: "remove tags 2" }));

    const data = await printFields();
    expect(data.tags).toEqual(["A", "C"]);
  });

});

describe("PrintForm empty template", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("posts data: {} for a single template declaring no parameters", async () => {
    const detail: TemplateDetail = {
    params: [],
      id: "no_params_tpl",
      name: "No Parameters",
      description: "",
      categories: [],
      unit: "mm",
      dpi: 300,
      format: { type: "single", width: 80, height: 24 },
      variables: [],
    };
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      void init;
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/printers")) {
        return new Response(JSON.stringify(printers), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/print")) {
        return new Response(JSON.stringify(summary), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/render/label")) {
        return new Response(new Blob(["img"]), {
          status: 200,
          headers: { "content-type": "image/png" },
        });
      }
      return new Response("{}", { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    renderForm(detail);
    await screen.findByText("Label Printer");
    fireEvent.change(await screen.findByLabelText("printer"), { target: { value: "p1" } });
    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);
    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(body.labels).toEqual([{ data: {}, copies: 1 }]);
    expect(body.fields).toBeUndefined();
  });
});

// #413: a boolean's published default is a JSON boolean (a tokened one already resolved by the
// server), so the checkbox starts there, else unchecked, and always sends its value.
describe("issue-413: two-state checkbox", () => {
  const booleans: TemplateDetail = {
    id: "bools",
    name: "Booleans",
    description: "",
    categories: [],
    unit: "mm",
    dpi: 300,
    format: { type: "single", width: 80, height: 24 },
    params: [
      { name: "plain", type: "boolean", control: "checkbox" },
      { name: "off", type: "boolean", control: "checkbox", default: false },
      { name: "on", type: "boolean", control: "checkbox", default: true },
      { name: "tokened", type: "boolean", control: "checkbox", default: true },
    ],
    variables: [],
  };

  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "/api/settings") {
        return new Response(JSON.stringify({ default_printer_id: { value: null, is_default: true } }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/printers")) {
        return new Response(JSON.stringify(printers), { status: 200, headers: { "content-type": "application/json" } });
      }
      if (url.startsWith("/api/render/label")) {
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      }
      if (url === "/api/print") {
        return new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("A19: starts each box at its published default, else unchecked, and submits every value", async () => {
    renderForm(booleans);
    const box = (name: string) => screen.getByRole("checkbox", { name }) as HTMLInputElement;
    await screen.findByRole("checkbox", { name: "plain" });
    expect(["plain", "off", "on", "tokened"].map((n) => box(n).checked)).toEqual([false, false, true, true]);
    expect(screen.queryByText(/unset/i)).toBeNull();
    expect(screen.queryByRole("checkbox", { name: /use default/i })).toBeNull();

    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).toBeEnabled());
    fireEvent.click(print);
    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const untouched = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(untouched.labels[0].data).toEqual({ plain: false, off: false, on: true, tokened: true });

    fireEvent.click(box("plain"));
    fireEvent.click(box("on"));
    fireEvent.click(print);
    await waitFor(() => expect(countCalls("/api/print")).toBe(2));
    const toggled = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(toggled.labels[0].data).toEqual({ plain: true, off: false, on: false, tokened: true });
  });
});

describe("PrintForm chain and continue", () => {
  const two = [
    { id: "p1", name: "Label Printer", uri: "ipp://p1/q", insecure: false },
    { id: "p2", name: "Backup Printer", uri: "ipp://p2/q", insecure: false },
  ];
  const supported = async () => json(probeOk(true));
  const chainBox = () => screen.queryByRole("checkbox", { name: "Chain and continue" });
  const findChainBox = () => screen.findByRole("checkbox", { name: "Chain and continue" });
  const bodyOf = (call: unknown[]) => JSON.parse((call[1] as RequestInit).body as string);
  const printBody = () => bodyOf(lastCall("/api/print")!);
  const ordinary = (printer: string, copies = 1) => ({
    template: tape.id,
    printer,
    labels: [{ data: { message: "hello" }, copies }],
  });
  const use = (mock: ReturnType<typeof stubFetch>) => {
    fetchMock = mock;
    vi.stubGlobal("fetch", fetchMock);
  };
  const clickPrint = async (n: number) => {
    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).toBeEnabled());
    fireEvent.click(print);
    await waitFor(() => expect(countCalls("/api/print")).toBe(n));
  };

  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("offers an unchecked box beside Print on confirmed support and sends true only when checked", async () => {
    use(stubFetch(printers, null, { p1: supported }));
    renderForm(tape);
    await fillAndSelectPrinter();
    const box = await findChainBox();
    expect(box).not.toBeChecked();
    expect(box).toHaveAccessibleDescription(/last one attached for the next print/);
    const print = screen.getByRole("button", { name: /^print$/i });
    expect(print.closest("div[class*='sticky']")).toContainElement(box);
    expect((lastCall("/api/printers/p1/probe")![1] as RequestInit).method).toBe("POST");

    await clickPrint(1);
    expect(printBody()).toEqual(ordinary("p1"));

    fireEvent.click(box);
    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "3" } });
    await clickPrint(2);
    expect(printBody()).toEqual({ ...ordinary("p1", 3), chain_and_continue: true });
    expect(box).toBeChecked();
  });

  it("hides the box while support is loading, unsupported or unreachable, keeping Print usable", async () => {
    const cases: Record<string, () => Promise<Response>> = {
      loading: () => new Promise<Response>(() => {}),
      unsupported: async () => json(probeOk(false)),
      unreachable: async () => json({ status: "unreachable", detail: "down" }),
      failed: async () => json({ error: { code: "Internal", message: "boom" } }, 500),
    };
    for (const [name, answer] of Object.entries(cases)) {
      use(stubFetch(printers, null, { p1: answer }));
      renderForm(tape);
      await fillAndSelectPrinter();
      await waitFor(() => expect(countCalls("/api/printers/p1/probe")).toBe(1));
      await clickPrint(1);
      expect(chainBox(), name).toBeNull();
      expect(printBody(), name).toEqual(ordinary("p1"));
      cleanup();
    }
  });

  it("hides the box and probes nothing with no printer selected", async () => {
    use(stubFetch(two, null, { p1: supported, p2: supported }));
    renderForm(tape);
    await screen.findByLabelText("printer");
    await new Promise((r) => setTimeout(r, 50));
    expect(chainBox()).toBeNull();
    expect(countCalls("/api/printers/p1/probe") + countCalls("/api/printers/p2/probe")).toBe(0);
  });

  it("clears the choice on a printer switch, even to a printer whose support is already known", async () => {
    use(stubFetch(two, null, { p1: supported, p2: supported }));
    renderForm(tape);
    const printer = await screen.findByLabelText("printer");
    await screen.findByRole("option", { name: "Backup Printer" });
    fireEvent.change(printer, { target: { value: "p2" } });
    await findChainBox();
    await fillAndSelectPrinter();
    fireEvent.click(await findChainBox());
    expect(chainBox()).toBeChecked();

    // p2's cached result confirms support at once, so only the switch itself clears the choice.
    fireEvent.change(printer, { target: { value: "p2" } });
    expect(chainBox()).not.toBeChecked();
    await clickPrint(1);
    expect(printBody()).toEqual(ordinary("p2"));
  });

  it("does not let the previous printer's late probe enable the box for the new one", async () => {
    let answerP1: (r: Response) => void = () => {};
    use(
      stubFetch(two, null, {
        p1: () => new Promise((r) => (answerP1 = r)),
        p2: async () => json(probeOk(false)),
      }),
    );
    renderForm(tape);
    await fillAndSelectPrinter();
    await waitFor(() => expect(countCalls("/api/printers/p1/probe")).toBe(1));
    fireEvent.change(screen.getByLabelText("printer"), { target: { value: "p2" } });
    await waitFor(() => expect(countCalls("/api/printers/p2/probe")).toBe(1));
    answerP1(json(probeOk(true)));
    await new Promise((r) => setTimeout(r, 50));
    expect(chainBox()).toBeNull();
    await clickPrint(1);
    expect(printBody()).toEqual(ordinary("p2"));
  });

  it("clears a checked choice once the printer's latest probe no longer confirms support", async () => {
    let answer = supported;
    use(stubFetch(printers, null, { p1: () => answer() }));
    const qc = renderForm(tape);
    await fillAndSelectPrinter();
    fireEvent.click(await findChainBox());

    const failed = async () => json({ error: { code: "Internal", message: "boom" } }, 500);
    for (const lost of [async () => json(probeOk(false)), failed]) {
      answer = lost;
      await qc.invalidateQueries({ queryKey: ["printers"] });
      await waitFor(() => expect(chainBox()).toBeNull());

      answer = supported;
      await qc.invalidateQueries({ queryKey: ["printers"] });
      const box = await findChainBox();
      expect(box).not.toBeChecked();
      fireEvent.click(box);
    }
    answer = async () => json(probeOk(false));
    await qc.invalidateQueries({ queryKey: ["printers"] });
    await waitFor(() => expect(chainBox()).toBeNull());
    await clickPrint(1);
    expect(printBody()).toEqual(ordinary("p1"));
  });

  it("shows an unsupported-continuation refusal through the print error presentation", async () => {
    const base = stubFetch(printers, null, { p1: supported });
    const refusal = {
      error: {
        code: "InvalidRequest",
        message: "chain_and_continue is not supported: the printer does not advertise it",
        details: { reason: "chain_and_continue_unsupported" },
      },
    };
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) =>
      String(input) === "/api/print" ? json(refusal, 400) : base(input, init),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderForm(tape);
    await fillAndSelectPrinter();
    fireEvent.click(await findChainBox());
    await clickPrint(1);
    expect(printBody().chain_and_continue).toBe(true);
    expect(await screen.findByText(refusal.error.message)).toBeInTheDocument();
  });

  it("keeps continuation out of preview and download requests", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    const cases = [
      { detail: tape, field: "download format", value: "pdf", extra: { format: "pdf" } },
      { detail: sheet, field: "start slot", value: "2", extra: { start_slot: 2 } },
    ];
    for (const { detail, field, value, extra } of cases) {
      use(stubFetch(printers, null, { p1: supported }));
      renderForm(detail);
      await fillAndSelectPrinter();
      fireEvent.click(await findChainBox());
      fireEvent.change(screen.getByLabelText(field), { target: { value } });
      await waitFor(() => expect(countCalls("/api/render/label") + countCalls("/api/render")).toBeGreaterThan(0));
      fireEvent.click(screen.getByRole("button", { name: /^download$/i }));

      // A download carries copies; a sheet preview through the same path does not.
      const downloads = () =>
        fetchMock.mock.calls.filter(([u]) => String(u) === "/api/render").map(bodyOf).filter((b) => b.labels[0].copies);
      await waitFor(() => expect(downloads()).toHaveLength(1));
      expect(downloads()[0]).toEqual({ template: detail.id, labels: [{ data: { message: "hello" }, copies: 1 }], ...extra });
      const renders = fetchMock.mock.calls.filter(([u]) => String(u).startsWith("/api/render"));
      expect(renders.length).toBeGreaterThan(1);
      for (const call of renders) expect(bodyOf(call)).not.toHaveProperty("chain_and_continue");
      cleanup();
    }
  });
});
