import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { PrintForm } from "./PrintForm";
import type { TemplateDetail } from "../../api/types";

const tape: TemplateDetail = {
  params: [],
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
  params: [],
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
      if (url.includes("types_tpl")) {
        return new Response(
          JSON.stringify({
            inputs: [
              [
                { name: "printed_on", control: "datetime", required: true },
                { name: "flag", control: "checkbox", required: true },
                { name: "choice", control: "select", values: ["one", "two"], required: true },
                { name: "token_field", control: "text", required: false },
                { name: "lit_field", control: "text", default: "seeded", required: false },
              ],
            ],
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      }
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
    expect(body.data).toEqual({ message: "hello" });
    expect(body.fields).toBeUndefined();
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

    // tier publishes a default, so it arrives deferred and disabled; clearing that is what puts the
    // operator's own choice into the request and into the submitted data.
    fireEvent.click(screen.getByRole("checkbox", { name: "Use default for tier" }));
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
    expect(body.data).toEqual({ message: "hello", tier: "standard" });
    expect(body.fields).toBeUndefined();
  });

  it("leaves undefaulted datetime, boolean, and enum empty on mount and seeds literal defaults", async () => {
    const detailWithTypes: TemplateDetail = {
    params: [],
      id: "types_tpl",
      name: "Types Template",
      description: "",
      unit: "mm",
      dpi: 300,
      format: { type: "single", width: 80, height: 24 },
      inputs: {
        all: [
          { name: "printed_on", control: "datetime", required: true },
          { name: "flag", control: "checkbox", required: true },
          { name: "choice", control: "select", values: ["one", "two"], required: true },
          { name: "token_field", control: "text", required: false },
          { name: "lit_field", control: "text", default: "seeded", required: false },
        ],
        default: [
          { name: "printed_on", control: "datetime", required: true },
          { name: "flag", control: "checkbox", required: true },
          { name: "choice", control: "select", values: ["one", "two"], required: true },
          { name: "token_field", control: "text", required: false },
          { name: "lit_field", control: "text", default: "seeded", required: false },
        ],
      },
      variables: [],
    };

    const fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);

    renderForm(detailWithTypes);

    // printed_on has no default -> value is ""
    const dtInput = (await screen.findByLabelText("printed_on")) as HTMLInputElement;
    expect(dtInput.value).toBe("");

    // flag has no default -> Unset
    expect(screen.getByText("Unset")).toBeInTheDocument();

    // choice has no default -> value is ""
    const choiceSelect = (await screen.findByLabelText("choice")) as HTMLSelectElement;
    expect(choiceSelect.value).toBe("");

    // token_field has no default -> value is ""
    const tokInput = (await screen.findByLabelText("token_field")) as HTMLInputElement;
    expect(tokInput.value).toBe("");

    // lit_field has default "seeded" -> value is "seeded"
    const litInput = (await screen.findByLabelText("lit_field")) as HTMLInputElement;
    expect(litInput.value).toBe("seeded");

    // Undefaulted required fields (printed_on, flag, choice) are demanded; form is invalid and Print is disabled
    const printBtn = screen.getByRole("button", { name: /^print$/i });
    expect(printBtn).toBeDisabled();

    // Fill in the required fields
    fireEvent.change(dtInput, { target: { value: "2026-08-19T14:30" } });
    fireEvent.change(choiceSelect, { target: { value: "one" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "flag" }));
    fireEvent.change(await screen.findByLabelText("printer"), { target: { value: "p1" } });

    // Now form is complete and Print button is enabled
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

  // A list request answers from whatever `data` it is given, so these stubs echo the branch the
  // request selects; the assertions read the request bodies themselves.
  function stubInputs(listFor: (data: Record<string, unknown>) => unknown[]) {
    const mock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/printers")) {
        return new Response(JSON.stringify(printers), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/templates/") && url.includes("/inputs")) {
        const parsed = init?.body ? JSON.parse(init.body as string) : null;
        return new Response(JSON.stringify({ inputs: [listFor(parsed?.labels?.[0]?.data ?? {})] }), {
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

  const firstInputsData = () => {
    const call = fetchMock.mock.calls.find(
      ([u]) => String(u).startsWith("/api/templates/") && String(u).includes("/inputs"),
    );
    return JSON.parse((call![1] as RequestInit).body as string).labels[0].data as Record<string, unknown>;
  };

  const lastInputsData = () => {
    const call = [...fetchMock.mock.calls]
      .reverse()
      .find(([u]) => String(u).startsWith("/api/templates/") && String(u).includes("/inputs"));
    return JSON.parse((call![1] as RequestInit).body as string).labels[0].data as Record<string, unknown>;
  };

  const printFields = async () => {
    const print = screen.getByRole("button", { name: /^print$/i });
    await waitFor(() => expect(print).not.toBeDisabled());
    const before = countCalls("/api/print");
    fireEvent.click(print);
    await waitFor(() => expect(countCalls("/api/print")).toBe(before + 1));
    const body = JSON.parse((lastCall("/api/print")![1] as RequestInit).body as string);
    expect(body.fields).toBeUndefined();
    return body.data as Record<string, unknown>;
  };

  const withInputs = (list: unknown[]): TemplateDetail => ({
    ...tape,
    id: "def_tpl",
    inputs: { all: list as TemplateDetail["inputs"]["all"], default: list as TemplateDetail["inputs"]["default"] },
  });

  it("omits a deferred name from the submitted data and from the list request, and sends it once cleared", async () => {
    const list = [
      { name: "message", control: "text", required: true },
      { name: "title", control: "text", default: "Untitled" },
    ];
    stubInputs(() => list);
    renderForm(withInputs(list));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });

    expect(await printFields()).toEqual({ message: "hello" });
    await waitFor(() => expect(lastInputsData()).toEqual({ message: "hello" }));

    fireEvent.click(screen.getByRole("checkbox", { name: "Use default for title" }));

    expect(await printFields()).toEqual({ message: "hello", title: "Untitled" });
    await waitFor(() => expect(lastInputsData()).toEqual({ message: "hello", title: "Untitled" }));
  });

  it("defers a published default no control can hold", async () => {
    const list = [
      { name: "message", control: "text", required: true },
      { name: "width", control: "number", default: "80mm" },
      { name: "logo", control: "image", default: "data:image/png;base64,AAAA" },
    ];
    stubInputs(() => list);
    renderForm(withInputs(list));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });

    for (const name of ["width", "logo"]) {
      const box = screen.getByRole("checkbox", { name: `Use default for ${name}` }) as HTMLInputElement;
      expect(box.checked).toBe(true);
      expect(screen.getByLabelText(name)).toBeDisabled();
    }
    const widthBox = screen.getByRole("checkbox", { name: "Use default for width" });
    expect(widthBox.closest("label")).toHaveTextContent("Use default: 80mm");

    expect(await printFields()).toEqual({ message: "hello" });
  });

  it("submits what the control holds once cleared, and discards it on re-checking", async () => {
    const list = [
      { name: "message", control: "text", required: true },
      { name: "title", control: "text", default: "Untitled" },
    ];
    stubInputs(() => list);
    renderForm(withInputs(list));

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
    const list = [
      { name: "message", control: "text", required: true },
      { name: "logo", control: "image", default: "data:image/png;base64,AAAA" },
    ];
    stubInputs(() => list);
    renderForm(withInputs(list));

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

  it("brings a later entry in deferred, and keeps a cleared one cleared across a branch switch", async () => {
    const base = [
      { name: "message", control: "text", required: true },
      { name: "tier", control: "select", values: ["standard", "pro"], required: true },
    ];
    const pro = [...base, { name: "pro_note", control: "text", default: "note" }];
    stubInputs((data) => (data.tier === "pro" ? pro : base));
    renderForm(withInputs(base));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });
    fireEvent.change(screen.getByLabelText("tier"), { target: { value: "pro" } });

    const box = (await screen.findByRole("checkbox", {
      name: "Use default for pro_note",
    })) as HTMLInputElement;
    expect(box.checked).toBe(true);
    expect(await printFields()).toEqual({ message: "hello", tier: "pro" });

    fireEvent.click(box);
    fireEvent.change(screen.getByLabelText("tier"), { target: { value: "standard" } });
    await waitFor(() => expect(screen.queryByLabelText("pro_note")).toBeNull());
    fireEvent.change(screen.getByLabelText("tier"), { target: { value: "pro" } });

    const returned = (await screen.findByRole("checkbox", {
      name: "Use default for pro_note",
    })) as HTMLInputElement;
    expect(returned.checked).toBe(false);
    expect(await printFields()).toEqual({ message: "hello", tier: "pro", pro_note: "note" });
  });

  // A parameter name reserves no words, so an entry may be called `constructor` or `__proto__`. Both
  // read as present on any `{}` through the prototype, and `__proto__` cannot even be assigned onto
  // one, so these assert the deferral and the submission the names would otherwise silently lose.
  const ownEntries = (o: Record<string, unknown>) =>
    Object.keys(o)
      .sort()
      .map((k) => [k, Object.getOwnPropertyDescriptor(o, k)!.value]);

  it("defers and submits entries named for Object.prototype members", async () => {
    const list = [
      { name: "constructor", control: "text", required: true },
      { name: "__proto__", control: "text", default: "proto-default" },
    ];
    stubInputs(() => list);
    renderForm(withInputs(list));

    // `constructor` is required and holds nothing: the prototype's own `constructor` must not read
    // as its value.
    const ctor = (await screen.findByLabelText("constructor")) as HTMLInputElement;
    expect(ctor.value).toBe("");
    expect(screen.queryByRole("checkbox", { name: "Use default for constructor" })).toBeNull();
    expect(screen.getByRole("button", { name: /^print$/i })).toBeDisabled();

    const protoBox = screen.getByRole("checkbox", { name: "Use default for __proto__" }) as HTMLInputElement;
    expect(protoBox.checked).toBe(true);
    const proto = screen.getByLabelText("__proto__") as HTMLInputElement;
    expect(proto).toBeDisabled();
    expect(proto.value).toBe("proto-default");

    fireEvent.change(ctor, { target: { value: "hello" } });

    expect(ownEntries(await printFields())).toEqual([["constructor", "hello"]]);
    await waitFor(() => expect(ownEntries(lastInputsData())).toEqual([["constructor", "hello"]]));

    fireEvent.click(protoBox);
    expect(screen.getByLabelText("__proto__")).not.toBeDisabled();

    expect(ownEntries(await printFields())).toEqual([
      ["__proto__", "proto-default"],
      ["constructor", "hello"],
    ]);
  });

  it("brings a later entry named for an Object.prototype member in deferred", async () => {
    const base = [
      { name: "message", control: "text", required: true },
      { name: "tier", control: "select", values: ["standard", "pro"], required: true },
    ];
    const pro = [
      ...base,
      { name: "constructor", control: "text", default: "ctor-default" },
      { name: "__proto__", control: "text", default: "proto-default" },
    ];
    stubInputs((data) => (data.tier === "pro" ? pro : base));
    renderForm(withInputs(base));

    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });
    fireEvent.change(screen.getByLabelText("tier"), { target: { value: "pro" } });

    for (const name of ["constructor", "__proto__"]) {
      const box = (await screen.findByRole("checkbox", {
        name: `Use default for ${name}`,
      })) as HTMLInputElement;
      expect(box.checked).toBe(true);
      const control = screen.getByLabelText(name) as HTMLInputElement;
      expect(control).toBeDisabled();
      expect(control.value).toBe(name === "constructor" ? "ctor-default" : "proto-default");
    }

    expect(ownEntries(await printFields())).toEqual([
      ["message", "hello"],
      ["tier", "pro"],
    ]);
  });

  it("does not seed the previous template's entries when the new template's list request fails", async () => {
    const listA = [
      { name: "message", control: "text", required: true },
      { name: "a_only", control: "text", default: "A-only" },
    ];
    const listB = [{ name: "message", control: "text", required: true }];

    const mock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/printers")) {
        return new Response(JSON.stringify(printers), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/templates/") && url.includes("/inputs")) {
        // Template B's very first list request fails, which clears `pending` while the form holds no
        // list of its own.
        if (url.includes("tpl_b")) return new Response("boom", { status: 500 });
        return new Response(JSON.stringify({ inputs: [listA] }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response("{}", { status: 200 });
    });
    fetchMock = mock as unknown as ReturnType<typeof stubFetch>;
    vi.stubGlobal("fetch", mock);

    const a: TemplateDetail = { ...withInputs(listA), id: "tpl_a" };
    const b: TemplateDetail = { ...withInputs(listB), id: "tpl_b" };
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const tree = (detail: TemplateDetail) => (
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <PrintForm detail={detail} />
        </ToastProvider>
      </QueryClientProvider>
    );
    const { rerender } = render(tree(a));

    expect(await screen.findByRole("checkbox", { name: "Use default for a_only" })).toBeInTheDocument();

    rerender(tree(b));

    await screen.findByText(/Failed to derive inputs \(500\)/);
    expect(screen.queryByLabelText("a_only")).toBeNull();
    expect(screen.queryByRole("checkbox", { name: "Use default for a_only" })).toBeNull();
    expect(screen.getByLabelText("message")).toBeInTheDocument();
  });

  it("carries neither value nor deferral across a template change", async () => {
    const listA = [
      { name: "message", control: "text", required: true },
      { name: "title", control: "text", default: "A-title" },
    ];
    const listB = [
      { name: "message", control: "text", required: true },
      { name: "title", control: "text", default: "B-title" },
    ];
    stubInputs((data) => ((data.title ?? "").toString().startsWith("B") ? listB : listA));

    const a: TemplateDetail = { ...withInputs(listA), id: "tpl_a" };
    const b: TemplateDetail = { ...withInputs(listB), id: "tpl_b" };
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
    const list = [
      { name: "message", control: "text", required: true },
      { name: "event_time", control: "datetime", default: "2026-03-24", required: false },
    ];
    stubInputs(() => list);
    const detail: TemplateDetail = { ...withInputs(list), id: "tpl_dt" };
    renderForm(detail);

    const input = (await screen.findByLabelText("event_time")) as HTMLInputElement;
    expect(input.value).toBe("2026-03-24T00:00");
  });

  it("submits untouched undefaulted list entry as empty array without touching editor", async () => {
    const list = [{ name: "tags", control: "list", required: true }];
    stubInputs(() => list);
    renderForm(withInputs(list));

    const data = await printFields();
    expect(data.tags).toEqual([]);
  });

  it("submits data with elements in row order after appending twice and typing", async () => {
    const list = [{ name: "tags", control: "list", required: true }];
    stubInputs(() => list);
    renderForm(withInputs(list));

    const addBtn = await screen.findByRole("button", { name: "add tags" });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 1" }), { target: { value: "A" } });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 2" }), { target: { value: "B" } });

    const data = await printFields();
    expect(data.tags).toEqual(["A", "B"]);
  });

  it("submits element left empty as empty string", async () => {
    const list = [{ name: "tags", control: "list", required: true }];
    stubInputs(() => list);
    renderForm(withInputs(list));

    const addBtn = await screen.findByRole("button", { name: "add tags" });
    fireEvent.click(addBtn);

    const data = await printFields();
    expect(data.tags).toEqual([""]);
  });

  it("submits data with reordered elements after moving elements", async () => {
    const list = [{ name: "tags", control: "list", required: true }];
    stubInputs(() => list);
    renderForm(withInputs(list));

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
    const list = [{ name: "tags", control: "list", required: true }];
    stubInputs(() => list);
    renderForm(withInputs(list));

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
    const list = [{ name: "tags", control: "list", default: ["CONSUMABLE"], required: false }];
    stubInputs(() => list);
    renderForm(withInputs(list));

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
    const list = [{ name: "tags", control: "list", default: ["CONSUMABLE"], required: false }];
    stubInputs(() => list);
    renderForm(withInputs(list));

    const checkbox = await screen.findByRole("checkbox", { name: "Use default for tags" });
    fireEvent.click(checkbox);

    const removeBtn = screen.getByRole("button", { name: "remove tags 1" });
    expect(removeBtn).not.toBeDisabled();
    fireEvent.click(removeBtn);

    const data = await printFields();
    expect(data.tags).toEqual([]);
  });

  it("renders empty operable editor for default_error list entry and submits empty array", async () => {
    const list = [
      {
        name: "tags",
        control: "list",
        required: true,
        default_error: { reason: "param_default_unresolvable", message: "Variable base not found" },
      },
    ];
    stubInputs(() => list);
    renderForm(withInputs(list));

    expect(screen.queryByRole("checkbox", { name: "Use default for tags" })).toBeNull();
    expect(await screen.findByText("Variable base not found")).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByRole("button", { name: "add tags" })).not.toBeDisabled();

    const data = await printFields();
    expect(data.tags).toEqual([]);
  });

  it("sends empty array in list request for untouched undefaulted entry and retains value across branch switches", async () => {
    const base = [
      { name: "tier", control: "select", values: ["standard", "pro"], required: true },
      { name: "tags", control: "list", required: true },
    ];
    const pro = [
      { name: "tier", control: "select", values: ["standard", "pro"], required: true },
      { name: "other", control: "text", required: true },
    ];
    stubInputs((data) => (data.tier === "pro" ? pro : base));
    renderForm(withInputs(base));

    await waitFor(() => expect(lastInputsData().tags).toEqual([]));

    const addBtn = await screen.findByRole("button", { name: "add tags" });
    fireEvent.click(addBtn);
    fireEvent.change(screen.getByRole("textbox", { name: "tags 1" }), { target: { value: "VIP" } });
    await waitFor(() => expect(lastInputsData().tags).toEqual(["VIP"]));

    // Switch branch away to pro
    fireEvent.change(screen.getByLabelText("tier"), { target: { value: "pro" } });
    await screen.findByLabelText("other");
    expect(screen.queryByRole("textbox", { name: "tags 1" })).toBeNull();

    // Switch back to standard
    fireEvent.change(screen.getByLabelText("tier"), { target: { value: "standard" } });
    const restoredInput = (await screen.findByRole("textbox", { name: "tags 1" })) as HTMLInputElement;
    expect(restoredInput.value).toBe("VIP");
  });

  it("carries empty array in the very first list request for untouched undefaulted entry", async () => {
    const list = [{ name: "tags", control: "list", required: true }];
    stubInputs(() => list);
    renderForm(withInputs(list));

    await waitFor(() => expect(firstInputsData().tags).toEqual([]));
  });

  it("submits empty array for list entry arriving in a later list without defaults without touching editor", async () => {
    const standard = [
      { name: "tier", control: "select", values: ["standard", "pro"], required: true },
    ];
    const pro = [
      { name: "tier", control: "select", values: ["standard", "pro"], required: true },
      { name: "tags", control: "list", required: true },
    ];
    stubInputs((data) => (data.tier === "pro" ? pro : standard));
    renderForm(withInputs(standard));

    await screen.findByLabelText("tier");
    expect(screen.queryByRole("button", { name: "add tags" })).toBeNull();

    // Switch tier to pro, which brings in tags (control: "list", required: true, no default)
    fireEvent.change(screen.getByLabelText("tier"), { target: { value: "pro" } });

    // tags editor appears
    await screen.findByRole("button", { name: "add tags" });

    // Submit without touching tags editor
    const data = await printFields();
    expect(data.tier).toBe("pro");
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

  it("posts data: {} for a single template reporting no inputs", async () => {
    const detail: TemplateDetail = {
    params: [],
      id: "no_inputs_tpl",
      name: "No Inputs",
      description: "",
      unit: "mm",
      dpi: 300,
      format: { type: "single", width: 80, height: 24 },
      inputs: { all: [], default: [] },
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
      if (url.startsWith("/api/templates/") && url.includes("/inputs")) {
        return new Response(JSON.stringify({ inputs: [[]] }), {
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
    expect(body.data).toEqual({});
    expect(Object.prototype.hasOwnProperty.call(body, "data")).toBe(true);
    expect(body.fields).toBeUndefined();
  });
});
