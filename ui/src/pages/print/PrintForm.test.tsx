import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
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

function stubFetch(printersList: unknown[] = printers, defaultPrinterId: string | null = null) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    void init;
    const url = typeof input === "string" ? input : input.toString();
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

  it("routes a tape Print to /api/print with the label repeated `copies` times", async () => {
    renderForm(tape);
    await fillAndSelectPrinter();

    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "3" } });

    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);

    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    const label = { data: { message: "hello" } };
    expect(body).toEqual({ template: tape.id, printer: "p1", labels: [label, label, label] });
    expect(countCalls("/api/render")).toBe(0);
    expect(await screen.findByText("Sent 1 labels to Label Printer")).toBeInTheDocument();
  });

  it("routes a sheet Print to /api/print with the label repeated `copies` times", async () => {
    renderForm(sheet);
    await fillAndSelectPrinter();

    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "2" } });

    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    fireEvent.click(print);

    await waitFor(() => expect(countCalls("/api/print")).toBe(1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(body.labels.length).toBe(2);
    expect(body.mode).toBeUndefined();
  });

  it("downloads a single template through /api/render with `copies` labels in the chosen format", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderForm(tape);
    const message = (await screen.findByLabelText("message")) as HTMLInputElement;
    fireEvent.change(message, { target: { value: "hello" } });
    fireEvent.change(screen.getByLabelText("copies"), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText("download format"), { target: { value: "pdf" } });

    fireEvent.click(screen.getByRole("button", { name: /^download$/i }));

    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const body = JSON.parse((lastCall("/api/render")![1] as RequestInit).body as string);
    const label = { data: { message: "hello" } };
    expect(body).toEqual({ template: tape.id, labels: [label, label], format: "pdf" });
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

  it("leaves undefaulted datetime, boolean, and enum empty on mount, seeds literal defaults, and gates only on a printer", async () => {
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
    expect((screen.getByLabelText("lit_field") as HTMLInputElement).value).toBe("seeded");

    // The form does not judge completeness: blank fields leave Download enabled, and Print waits
    // only for a printer.
    const printBtn = screen.getByRole("button", { name: /^print$/i });
    expect(screen.getByRole("button", { name: /^download$/i })).toBeEnabled();
    expect(printBtn).toBeDisabled();
    fireEvent.change(await screen.findByLabelText("printer"), { target: { value: "p1" } });
    await waitFor(() => expect(printBtn).not.toBeDisabled());
  });
});

describe("PrintForm deferring to a declared default", () => {
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

  it("omits a deferred name from the submitted data, and sends it once cleared", async () => {
    const list: Param[] = [
      { name: "message", type: "string", control: "text" },
      { name: "title", type: "string", control: "text", default: "Untitled" },
    ];
    stubParams();
    renderForm(withParams(list));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });

    expect(await printFields()).toEqual({ message: "hello" });

    fireEvent.click(screen.getByRole("checkbox", { name: "Use default for title" }));

    expect(await printFields()).toEqual({ message: "hello", title: "Untitled" });
  });

  it("defers a published default no control can hold", async () => {
    const list: Param[] = [
      { name: "message", type: "string", control: "text" },
      { name: "width", type: "number", control: "number", default: 80 },
      { name: "logo", type: "string", control: "image", default: "data:image/png;base64,AAAA" },
    ];
    stubParams();
    renderForm(withParams(list));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });

    for (const name of ["width", "logo"]) {
      const box = screen.getByRole("checkbox", { name: `Use default for ${name}` }) as HTMLInputElement;
      expect(box.checked).toBe(true);
      expect(screen.getByLabelText(name)).toBeDisabled();
    }
    const widthBox = screen.getByRole("checkbox", { name: "Use default for width" });
    expect(widthBox.closest("label")).toHaveTextContent("Use default: 80");

    expect(await printFields()).toEqual({ message: "hello" });
  });

  it("submits what the control holds once cleared, and discards it on re-checking", async () => {
    const list: Param[] = [
      { name: "message", type: "string", control: "text" },
      { name: "title", type: "string", control: "text", default: "Untitled" },
    ];
    stubParams();
    renderForm(withParams(list));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });
    const box = screen.getByRole("checkbox", { name: "Use default for title" });
    fireEvent.click(box);

    const title = screen.getByLabelText("title") as HTMLInputElement;
    expect(title).not.toBeDisabled();
    fireEvent.change(title, { target: { value: "Kitchen" } });
    expect(await printFields()).toEqual({ message: "hello", title: "Kitchen" });

    fireEvent.click(box);
    expect(screen.getByLabelText("title")).toBeDisabled();
    expect((screen.getByLabelText("title") as HTMLInputElement).value).toBe("Untitled");
    expect(await printFields()).toEqual({ message: "hello" });
  });

  it("clears the file chooser's own selection when an image entry is re-checked", async () => {
    const list: Param[] = [
      { name: "message", type: "string", control: "text" },
      { name: "logo", type: "string", control: "image", default: "data:image/png;base64,AAAA" },
    ];
    stubParams();
    renderForm(withParams(list));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });
    const box = screen.getByRole("checkbox", { name: "Use default for logo" });
    fireEvent.click(box);

    const chooser = screen.getByLabelText("logo") as HTMLInputElement;
    const file = new File(["png-bytes"], "logo.png", { type: "image/png" });
    fireEvent.change(chooser, { target: { files: [file] } });
    await waitFor(() => expect(screen.getByText("image selected")).toBeInTheDocument());

    // jsdom leaves `files` in place, so the chooser's own value is what shows the reset.
    Object.defineProperty(chooser, "value", {
      value: "C:\\fakepath\\logo.png",
      writable: true,
      configurable: true,
    });

    fireEvent.click(box);
    expect(chooser.value).toBe("");
    expect(await printFields()).toEqual({ message: "hello" });
  });

  // A parameter name reserves no words, so one may be called `constructor` or `__proto__`. Both read
  // as present on any `{}` through the prototype, and `__proto__` cannot even be assigned onto one, so
  // this asserts the deferral and the submission the names would otherwise silently lose.
  const ownEntries = (o: Record<string, unknown>) =>
    Object.keys(o)
      .sort()
      .map((k) => [k, Object.getOwnPropertyDescriptor(o, k)!.value]);

  it("defers and submits entries named for Object.prototype members", async () => {
    const list: Param[] = [
      { name: "constructor", type: "string", control: "text" },
      { name: "__proto__", type: "string", control: "text", default: "proto-default" },
    ];
    stubParams();
    renderForm(withParams(list));

    // `constructor` holds nothing: the prototype's own `constructor` must not read as its value.
    const ctor = (await screen.findByLabelText("constructor")) as HTMLInputElement;
    expect(ctor.value).toBe("");
    expect(screen.queryByRole("checkbox", { name: "Use default for constructor" })).toBeNull();

    const protoBox = screen.getByRole("checkbox", { name: "Use default for __proto__" }) as HTMLInputElement;
    expect(protoBox.checked).toBe(true);
    const proto = screen.getByLabelText("__proto__") as HTMLInputElement;
    expect(proto).toBeDisabled();
    expect(proto.value).toBe("proto-default");

    fireEvent.change(ctor, { target: { value: "hello" } });

    expect(ownEntries(await printFields())).toEqual([["constructor", "hello"]]);

    fireEvent.click(protoBox);
    expect(screen.getByLabelText("__proto__")).not.toBeDisabled();

    expect(ownEntries(await printFields())).toEqual([
      ["__proto__", "proto-default"],
      ["constructor", "hello"],
    ]);
  });

  it("carries neither value nor deferral across a template change", async () => {
    const listA: Param[] = [
      { name: "message", type: "string", control: "text" },
      { name: "title", type: "string", control: "text", default: "A-title" },
    ];
    const listB: Param[] = [
      { name: "message", type: "string", control: "text" },
      { name: "title", type: "string", control: "text", default: "B-title" },
    ];
    stubParams();

    const a: TemplateDetail = { ...withParams(listA), id: "tpl_a" };
    const b: TemplateDetail = { ...withParams(listB), id: "tpl_b" };
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const tree = (detail: TemplateDetail) => (
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <PrintForm detail={detail} />
        </ToastProvider>
      </QueryClientProvider>
    );
    const { rerender } = render(tree(a));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Use default for title" }));
    fireEvent.change(screen.getByLabelText("title"), { target: { value: "Edited" } });

    rerender(tree(b));

    const box = (await screen.findByRole("checkbox", {
      name: "Use default for title",
    })) as HTMLInputElement;
    expect(box.checked).toBe(true);
    const title = screen.getByLabelText("title") as HTMLInputElement;
    expect(title).toBeDisabled();
    expect(title.value).toBe("B-title");
    expect(screen.getByLabelText("message")).toHaveValue("");
  });

  it("widens a bare YYYY-MM-DD default to YYYY-MM-DDT00:00 for datetime control", async () => {
    const list: Param[] = [
      { name: "message", type: "string", control: "text" },
      { name: "event_time", type: "datetime", control: "datetime", default: "2026-03-24" },
    ];
    stubParams();
    const detail: TemplateDetail = { ...withParams(list), id: "tpl_dt" };
    renderForm(detail);

    const input = (await screen.findByLabelText("event_time")) as HTMLInputElement;
    expect(input.value).toBe("2026-03-24T00:00");
  });

  it("submits untouched undefaulted list entry as empty array without touching editor", async () => {
    const list: Param[] = [{ name: "tags", type: "list", control: "list" }];
    stubParams();
    renderForm(withParams(list));

    const data = await printFields();
    expect(data.tags).toEqual([]);
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

  it("opens a defaulted list entry with one row, all controls disabled, checkbox checked, and sends no tags key", async () => {
    const list: Param[] = [{ name: "tags", type: "list", control: "list", default: ["CONSUMABLE"] }];
    stubParams();
    renderForm(withParams(list));

    const checkbox = (await screen.findByRole("checkbox", { name: "Use default for tags" })) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
    expect(screen.getByText("CONSUMABLE")).toBeInTheDocument();

    const input1 = screen.getByRole("textbox", { name: "tags 1" }) as HTMLInputElement;
    expect(input1.value).toBe("CONSUMABLE");
    expect(input1).toBeDisabled();
    expect(screen.getByRole("button", { name: "add tags" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "remove tags 1" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "move tags 1 earlier" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "move tags 1 later" })).toBeDisabled();

    const data = await printFields();
    expect(data.tags).toBeUndefined();
  });

  it("clearing default checkbox makes controls operable and removing row submits empty array", async () => {
    const list: Param[] = [{ name: "tags", type: "list", control: "list", default: ["CONSUMABLE"] }];
    stubParams();
    renderForm(withParams(list));

    const checkbox = await screen.findByRole("checkbox", { name: "Use default for tags" });
    fireEvent.click(checkbox);

    const removeBtn = screen.getByRole("button", { name: "remove tags 1" });
    expect(removeBtn).not.toBeDisabled();
    fireEvent.click(removeBtn);

    const data = await printFields();
    expect(data.tags).toEqual([]);
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
    expect(body.labels).toEqual([{ data: {} }]);
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
