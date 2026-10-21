import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { createMemoryRouter, Outlet, RouterProvider, useNavigate } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { parse } from "yaml";
import { ToastProvider } from "../app/toast";
import { TemplateDetail } from "./TemplateDetail";
import { SHEET_ICON, SINGLE_ICON, iconGeometry, noBadgeStyling } from "../setupTests";

const detail = {
  id: "brother_24mm_qr",
  name: "Brother 24mm Continuous Label",
  description: "Continuous label roll (24mm width)",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: { min: 10, max: 120 }, height: 24 },
  params: [
    { name: "code", type: "string", control: "text" },
    { name: "message", type: "string", control: "text" },
  ],
};

const source = "id: brother_24mm_qr\nname: Brother 24mm Continuous Label\n";

// Two printers with no default, so the preselect falls through to "none" and Print stays gated on
// an explicit printer selection.
const printers = [
  { id: "p1", name: "Label Printer", uri: "ipp://p1/q", insecure: false },
  { id: "p2", name: "Backup Printer", uri: "ipp://p2/q", insecure: false },
];

const other = { ...detail, id: "other_label", name: "Other Label" };
const otherSource = "id: other_label\nname: Other Label\n";

// What GET /source currently returns. A PUT overwrites it, so the stub models the server: the bytes
// the client sent are what a later read gets back.
let currentSource = source;

function stubFetch(deleteStatus = 204, putStatus = 200, sourceStatus = 200, slowOtherDetail = false) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (init?.method === "DELETE") {
        return deleteStatus === 204
          ? new Response(null, { status: 204 })
          : new Response(
              JSON.stringify({ error: { code: "Internal", message: "delete failed" } }),
              { status: deleteStatus, headers: { "content-type": "application/json" } },
            );
      }
      if (init?.method === "PUT") {
        // Every branch persists, because the case worth stubbing is the one where the write landed:
        // the server writes before it reloads, so a failed reload leaves the edit on disk. Since
        // #181 that failure is a 500 (only I/O still fails a reload); a 422 comes from the body
        // being rejected before anything is written, and no test here depends on that branch.
        currentSource = String(init.body);
        return putStatus === 200
          ? new Response(JSON.stringify(detail), {
              status: 200,
              headers: { "content-type": "application/json" },
            })
          : new Response(
              JSON.stringify({
                error: { code: "TemplateInvalid", message: "layout[0].size: out of bounds" },
              }),
              { status: putStatus, headers: { "content-type": "application/json" } },
            );
      }
      if (url.endsWith("/api/templates/other_label/source")) {
        return new Response(otherSource, { status: 200, headers: { "content-type": "text/yaml" } });
      }
      if (url.endsWith("/api/templates/other_label")) {
        // Optionally slow: useTemplate keeps the previous detail as placeholder data, so this window
        // is exactly when the page shows template A's detail under template B's URL.
        if (slowOtherDetail) await new Promise((r) => setTimeout(r, 300));
        return new Response(JSON.stringify(other), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.endsWith("/api/templates/brother_24mm_qr/source")) {
        return sourceStatus === 200
          ? new Response(currentSource, { status: 200, headers: { "content-type": "text/yaml" } })
          : new Response("nope", { status: sourceStatus });
      }
      if (url.endsWith("/api/templates/brother_24mm_qr")) {
        return new Response(JSON.stringify(detail), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/printers")) {
        return new Response(JSON.stringify(printers), { status: 200, headers: { "content-type": "application/json" } });
      }
      if (url === "/api/settings") {
        return new Response(JSON.stringify({ default_printer_id: { value: null, is_default: true } }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/render/label")) {
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }),
  );
}

function GoToOther() {
  const navigate = useNavigate();
  return (
    <button type="button" onClick={() => navigate("/templates/other_label")}>
      go to other
    </button>
  );
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // A data router, not a declarative MemoryRouter: the YAML guard's useBlocker needs its context.
  const router = createMemoryRouter(
    [
      {
        element: (
          <>
            <GoToOther />
            <Outlet />
          </>
        ),
        children: [
          { path: "/templates/:id", element: <TemplateDetail /> },
          // The list is the app's index route; "/templates" is only a redirect to it.
          { path: "/", element: <div>Labels list</div> },
        ],
      },
    ],
    { initialEntries: ["/templates/brother_24mm_qr"] },
  );
  const view = render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <RouterProvider router={router} />
      </ToastProvider>
    </QueryClientProvider>,
  );
  return { ...view, router, qc };
}

const cloneSource = '\uFEFF# original\nname: Pallet  # display name\ndescription: \'Keep this\'\nunit: mm\nwidth: 80\n';
const firstCopy = '\uFEFF# original\nname: "Pallet (copy)"  # display name\ndescription: \'Keep this\'\nunit: mm\nwidth: 80\n';
const secondCopy = '\uFEFF# original\nname: "Pallet (copy 2)"  # display name\ndescription: \'Keep this\'\nunit: mm\nwidth: 80\n';

function stubCloneFetch(createStatus = 201, sourceStatus = 200) {
  stubFetch();
  const fallback = globalThis.fetch;
  const sources = new Map([[detail.id, cloneSource]]);
  const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const match = url.match(/^\/api\/templates\/([^/]+)(\/source)?$/);
    if (!match) return fallback(input, init);
    const [, id, sourcePath] = match;
    if (init?.method === "POST") {
      const status = sources.has(id) ? 409 : createStatus;
      if (status !== 201) {
        return new Response(JSON.stringify({ error: { code: "CreateFailed", message: `create ${status}` } }), {
          status, headers: { "content-type": "application/json" },
        });
      }
      sources.set(id, String(init.body));
    }
    if (init?.method === "PUT") sources.set(id, String(init.body));
    const yaml = sources.get(id);
    if (yaml === undefined) return fallback(input, init);
    if (sourcePath) return new Response(yaml, { status: sourceStatus });
    return new Response(JSON.stringify({ ...detail, id, name: parse(yaml).name }), {
      status: init?.method === "POST" ? 201 : 200,
      headers: { "content-type": "application/json" },
    });
  });
  vi.stubGlobal("fetch", fetch);
  return { fetch, sources };
}

describe("Template cloning", () => {
  it("creates distinct copies through collisions and opens a clean, seeded editor", async () => {
    const { fetch, sources } = stubCloneFetch();
    const { router } = renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Clone" }));
    expect(screen.getByRole("button", { name: "Clone" })).toBeDisabled();
    expect(await screen.findByRole("textbox", { name: "Template YAML" })).toHaveValue(firstCopy);
    expect(screen.getByText("Raw YAML").closest("details")).toHaveAttribute("open");
    expect(router.state.location.pathname).toBe(`/templates/${detail.id}-copy`);
    expect(router.state.location.state).toEqual({ openForEditing: true });
    await waitFor(() => expect(screen.getByRole("button", { name: "Clone" })).toBeEnabled());

    await act(() => router.navigate(-1));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await screen.findByRole("heading", { name: "Pallet" });
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(await screen.findByRole("textbox", { name: "Template YAML" })).toHaveValue(secondCopy);
    expect(router.state.location.pathname).toBe(`/templates/${detail.id}-copy-2`);
    expect(sources.get(`${detail.id}-copy`)).toBe(firstCopy);
    expect(sources.get(`${detail.id}-copy-2`)).toBe(secondCopy);
    expect(sources.get(detail.id)).toBe(cloneSource);

    const writes = fetch.mock.calls.filter(([url, init]) => String(url).startsWith("/api/templates/") && init?.method);
    expect(writes.map(([url, init]) => [url, init?.method, init?.body])).toEqual([
      [`/api/templates/${detail.id}-copy`, "POST", firstCopy],
      [`/api/templates/${detail.id}-copy`, "POST", firstCopy],
      [`/api/templates/${detail.id}-copy-2`, "POST", secondCopy],
    ]);
  });

  it("revisits a saved copy using current source with a clean baseline", async () => {
    const { sources } = stubCloneFetch();
    const { router, qc } = renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Clone" }));
    const editor = await screen.findByRole("textbox", { name: "Template YAML" });
    const saved = firstCopy.replace('name: "Pallet (copy)"', 'name: "Saved copy"');
    fireEvent.change(editor, { target: { value: saved } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByRole("button", { name: "Edit" });
    expect(sources.get(`${detail.id}-copy`)).toBe(saved);
    fireEvent.click(screen.getByRole("button", { name: "go to other" }));
    await screen.findByRole("heading", { name: "Other Label" });
    // Evict the source to exercise a history revisit that must fetch before seeding the editor.
    qc.removeQueries({ queryKey: ["template-source", `${detail.id}-copy`] });
    await act(() => router.navigate(-1));
    expect(await screen.findByRole("textbox", { name: "Template YAML" })).toHaveValue(saved);
    expect(screen.getByRole("button", { name: "Clone" })).toBeEnabled();
    const unload = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "go to other" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await screen.findByRole("heading", { name: "Other Label" });
  });

  it.each(["save", "discard"])("disables Clone for an unsaved edit until %s", async (action) => {
    const { fetch } = stubCloneFetch();
    renderPage();
    fireEvent.click(await screen.findByText("Raw YAML"));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Template YAML" }), {
      target: { value: cloneSource + "# edit\n" },
    });
    expect(screen.getByRole("button", { name: "Clone" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(fetch.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(0);
    if (action === "save") {
      fireEvent.click(screen.getByRole("button", { name: "Save" }));
    } else {
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
      fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    }
    await waitFor(() => expect(screen.getByRole("button", { name: "Clone" })).toBeEnabled());
  });

  it.each([
    { failure: "source", createStatus: 201, sourceStatus: 500, message: "source 500", posts: 0 },
    { failure: "create", createStatus: 500, sourceStatus: 200, message: "create 500", posts: 1 },
  ])("keeps a $failure failure visible on the original without retrying create", async ({ createStatus, sourceStatus, message, posts }) => {
    const { fetch, sources } = stubCloneFetch(createStatus, sourceStatus);
    const { router } = renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Clone" }));
    expect(await screen.findByText(message)).toBeVisible();
    expect(screen.getByRole("status").textContent).toBe(message);
    expect(router.state.location.pathname).toBe(`/templates/${detail.id}`);
    expect(screen.getByRole("heading", { name: "Pallet" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Template YAML" })).not.toBeInTheDocument();
    expect(fetch.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(posts);
    expect(fetch.mock.calls.filter(([, init]) => init?.method === "PUT" || init?.method === "DELETE")).toHaveLength(0);
    expect([...sources.entries()]).toEqual([[detail.id, cloneSource]]);
    expect(screen.getByRole("button", { name: "Clone" })).toBeEnabled();
  });

  it("sends no create if the fetched source cannot be parsed", async () => {
    const { fetch, sources } = stubCloneFetch();
    const { router } = renderPage();
    await screen.findByRole("heading", { name: "Pallet" });
    sources.set(detail.id, "name: [broken\n");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(await screen.findByText(/end with a \]/)).toBeVisible();
    expect(router.state.location.pathname).toBe(`/templates/${detail.id}`);
    expect(fetch.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(0);
  });
});

// A sheet detail, with its own stub and route, so the twenty tests built around the single fixture
// above are untouched. Without a sheet here the detail page could keep an iconless, countless pill
// and every suite would still pass, since the grid is the only place a sheet badge was rendered.
const sheetDetail = {
  id: "avery5163",
  name: "Avery 5163",
  description: "Shipping labels",
  unit: "in",
  dpi: 300,
  format: {
    type: "sheet",
    paper_width: 8.5,
    paper_height: 11,
    label_width: 4,
    label_height: 2,
    positions: [
      [0, 0],
      [4.25, 0],
      [0, 2],
      [4.25, 2],
      [0, 4],
      [4.25, 4],
    ],
  },
  params: [{ name: "message", type: "string", control: "text" }],
};

function stubSheetFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/api/templates/avery5163/source")) {
        return new Response("id: avery5163\n", {
          status: 200,
          headers: { "content-type": "text/yaml" },
        });
      }
      if (url.endsWith("/api/templates/avery5163")) {
        return new Response(JSON.stringify(sheetDetail), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/printers")) {
        return new Response(JSON.stringify(printers), { status: 200, headers: { "content-type": "application/json" } });
      }
      if (url === "/api/settings") {
        return new Response(JSON.stringify({ default_printer_id: { value: null, is_default: true } }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/render/label")) {
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }),
  );
}

function renderSheetPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createMemoryRouter([{ path: "/templates/:id", element: <TemplateDetail /> }], {
    initialEntries: ["/templates/avery5163"],
  });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <RouterProvider router={router} />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Template detail, sheet format", () => {
  beforeEach(() => {
    stubSheetFetch();
  });

  // The same four assertions Templates.test.tsx makes against the grid card. Both pages must render
  // one badge (#201), and text alone would pass for a differently-drawn or differently-coloured one.
  it("renders the sheet badge with its icon geometry and its own colour tokens", async () => {
    renderSheetPage();
    await screen.findByText("Avery 5163");
    const badge = document.querySelector<HTMLElement>('[data-format="sheet"]')!;
    expect(badge.textContent).toBe("sheet \u00b7 6");
    expect(badge.style.color).toBe("var(--info)");
    expect(badge.style.background).toBe("var(--info-soft)");
    expect(badge.style.borderColor).toBe("var(--info)");
    expect(iconGeometry(badge)).toEqual(SHEET_ICON);
    // One badge on the page, not merely one correct badge: a leftover pill would pass the rest.
    expect(document.querySelectorAll("[data-format]")).toHaveLength(1);
  });

  // The Dimensions row's sentence ends in the word "sheet". It is prose about size, one row below
  // the badge, and #201 leaves it alone: badging it would say the same thing twice.
  it("leaves the Dimensions sentence as prose", async () => {
    renderSheetPage();
    await screen.findByText("Avery 5163");
    const dimensions = screen.getByText(/on 8.5 . 11 in sheet$/);
    expect(dimensions).not.toHaveAttribute("data-format");
    expect(dimensions.querySelector("svg")).toBeNull();
    expect(dimensions.querySelector("[data-format]")).toBeNull();
    expect(dimensions.textContent).not.toMatch(/\u00b7 6/);
    expect(noBadgeStyling(dimensions)).toBe(true);
  });
});

describe("Template detail", () => {
  // Wrapped, not passed by reference: beforeEach hands the hook a test context, which would land in
  // stubFetch's deleteStatus parameter.
  beforeEach(() => {
    currentSource = source;
    stubFetch();
  });

  it("renders the name and the format badge", async () => {
    renderPage();
    expect(await screen.findByText("Brother 24mm Continuous Label")).toBeInTheDocument();
    expect(screen.getByText("single")).toBeInTheDocument();
    // The same badge the grid renders, not just the same word: without the icon and the tokens this
    // page could keep an iconless pill and the sheet parity assertions would still pass.
    const badge = document.querySelector<HTMLElement>('[data-format="single"]')!;
    expect(badge.style.color).toBe("var(--accent)");
    expect(badge.style.background).toBe("var(--accent-soft)");
    expect(badge.style.borderColor).toBe("var(--accent)");
    expect(iconGeometry(badge)).toEqual(SINGLE_ICON);
    expect(document.querySelectorAll("[data-format]")).toHaveLength(1);
  });

  it("reveals the raw YAML source when toggled", async () => {
    renderPage();
    await screen.findByText("Brother 24mm Continuous Label");
    const toggle = await screen.findByText(/raw yaml/i);
    fireEvent.click(toggle);
    expect(await screen.findByText(/id: brother_24mm_qr/)).toBeInTheDocument();
  });

  it("deletes after confirming, then returns to the list", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    expect(await screen.findByText("Labels list")).toBeInTheDocument();
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    expect(
      calls.some(
        ([url, init]) =>
          String(url).endsWith("/api/templates/brother_24mm_qr") && init?.method === "DELETE",
      ),
    ).toBe(true);
    expect(await screen.findByText(/Deleted brother_24mm_qr/)).toBeInTheDocument();
  });

  it("cancelling sends no request", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls.some(([, init]) => init?.method === "DELETE")).toBe(false);
  });

  it("saves an edit through PUT and leaves edit mode", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Edited\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByRole("button", { name: "Edit" })).toBeInTheDocument();
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const put = calls.find(([, init]) => init?.method === "PUT");
    expect(String(put?.[1]?.body)).toContain("name: Edited");
  });

  it("A save reaches the preview", async () => {
    renderPage();
    const field = (await screen.findByLabelText("code")) as HTMLInputElement;
    fireEvent.change(field, { target: { value: "A" } });

    const renderCalls = () =>
      (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls.filter(([u]) =>
        String(u).startsWith("/api/render/label"),
      );
    await waitFor(() => expect(renderCalls().length).toBeGreaterThan(0));
    const before = renderCalls().length;

    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Edited\n" },
    });
    // dataUpdatedAt has millisecond resolution; make sure the refetch lands in a later millisecond.
    await new Promise((r) => setTimeout(r, 5));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    // The save refetches the template: a new dataUpdatedAt must reach the preview as a new key,
    // or it keeps showing the render of the previous version.
    await waitFor(() => expect(renderCalls().length).toBeGreaterThan(before));
    expect(screen.getByLabelText("code")).toHaveValue("A");
  });

  it("keeps the draft and shows the server message when the save fails", async () => {
    stubFetch(204, 422);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Broken\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    // Twice by design: inline beside the textarea, and in the toast.
    expect(await screen.findAllByText(/layout\[0\]\.size: out of bounds/)).toHaveLength(2);
    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(
      "id: brother_24mm_qr\nname: Broken\n",
    );
  });

  it("re-entering edit after a save shows the saved text", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Saved\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));

    // Fails with a bare invalidation: the stale cached source wins the race and reseeds pre-save text.
    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(
      "id: brother_24mm_qr\nname: Saved\n",
    );
  });

  it("after a failed save, the next edit seeds from what is actually stored", async () => {
    // The persisted-but-failed case: the stub kept the submitted bytes (the write landed) and still
    // returned 500 (the reload hit an unreadable directory), so the cached source is now wrong in
    // the other direction.
    stubFetch(204, 500);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Persisted\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findAllByText(/out of bounds/);

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));

    // Content, not fetch counts: removeQueries refetches on the same re-render the error handler
    // causes, so a before/after tally races the behavior it is meant to check.
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(
        "id: brother_24mm_qr\nname: Persisted\n",
      ),
    );
  });

  it("cancelling a modified draft asks before discarding", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "changed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));

    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue("changed");
  });

  it("drops an open draft when the route moves to another template", async () => {
    // The new detail is slow on purpose: `useTemplate` serves the old one as placeholder data
    // meanwhile, so keying the editor on detail.id would keep the previous template's draft mounted
    // and savable while the URL already points at the new one.
    stubFetch(204, 200, 200, true);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Mine\n" },
    });

    // React Router reuses the templates/:id component, so without a reset the draft would follow.
    // The dirty draft blocks the move first; Discard lets it through.
    fireEvent.click(screen.getByRole("button", { name: /go to other/i }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));

    // Immediately, while the new detail is still loading: the draft must already be gone.
    expect(screen.queryByRole("textbox", { name: /template yaml/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clone" })).toBeDisabled();
    expect(await screen.findByText("Other Label")).toBeInTheDocument();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(otherSource);
  });

  it("keeps a dirty draft on Keep editing, then reaches the destination on Discard", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Dirty\n" },
    });

    fireEvent.click(screen.getByRole("button", { name: /go to other/i }));
    expect(await screen.findByRole("alertdialog", { name: /unsaved yaml changes/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));

    // Staying put: the dialog is gone and the edited text survives.
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(
      "id: brother_24mm_qr\nname: Dirty\n",
    );

    fireEvent.click(screen.getByRole("button", { name: /go to other/i }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    expect(await screen.findByText("Other Label")).toBeInTheDocument();
  });

  it("does not ask when leaving with unchanged or reverted text", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));

    // Entering edit mode seeds draft from baseline, so it is clean; change then revert explicitly.
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Temporary\n" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), { target: { value: source } });

    fireEvent.click(screen.getByRole("button", { name: /go to other/i }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(await screen.findByText("Other Label")).toBeInTheDocument();
  });

  it("does not ask after a successful save", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Saved\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByRole("button", { name: "Edit" });

    fireEvent.click(screen.getByRole("button", { name: /go to other/i }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(await screen.findByText("Other Label")).toBeInTheDocument();
  });

  it("keeps protecting a failed save", async () => {
    stubFetch(204, 422);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Broken\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findAllByText(/out of bounds/);

    fireEvent.click(screen.getByRole("button", { name: /go to other/i }));
    expect(await screen.findByRole("alertdialog", { name: /unsaved yaml changes/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(
      "id: brother_24mm_qr\nname: Broken\n",
    );
  });

  it("disables Edit and shows an error when the source cannot be loaded", async () => {
    stubFetch(204, 200, 500);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    expect(await screen.findByText(/could not load the template source/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit" })).toBeDisabled();
  });

  it("keeps the user on the page when the delete fails", async () => {
    stubFetch(500);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    expect(await screen.findByText(/delete failed/i)).toBeInTheDocument();
    expect(screen.queryByText("Labels list")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
  });
});

// #413: the template page shows each parameter's published default, resolved of tokens by the server.
describe("issue-413: template page", () => {
  const published = {
    id: "brother_24mm_qr",
    name: "Brother 24mm Continuous Label",
    description: "",
    categories: [],
    unit: "mm",
    dpi: 300,
    format: { type: "single", width: 80, height: 24 },
    params: [
      { name: "url", type: "string", control: "text", default: "https://ex.co/" },
      { name: "bold", type: "boolean", control: "checkbox", default: false },
    ],
    variables: ["qr_base_url"],
  };

  function stubPublished(body: unknown) {
    const fn = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/source")) return new Response("id: x\n", { status: 200, headers: { "content-type": "text/yaml" } });
      if (/\/api\/templates\/[^/]+$/.test(url)) {
        return new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });
      }
      return new Response(new Blob(["x"]), { status: 200, headers: { "content-type": "image/png" } });
    });
    vi.stubGlobal("fetch", fn);
    return fn;
  }

  it("shows each published default by value, with no Referenced fields section", async () => {
    stubPublished(published);
    renderPage();
    // The form field's hint, resolved by the server: a tokened default arrives as its value.
    expect(await screen.findByText("default: https://ex.co/")).toBeInTheDocument();
    expect(screen.queryByText(/referenced fields/i)).toBeNull();
    expect(screen.queryByText(/resolved:/i)).toBeNull();
  });
});

// The print form lives on this page (it used to be the separate /print/:id screen).
describe("Template page print form", () => {
  const t1 = {
    id: "t1",
    name: "Tag",
    description: "",
    unit: "mm",
    dpi: 300,
    format: { type: "single" as const, width: 80, height: 24 },
    params: [{ name: "message", type: "string" as const, control: "text" as const }],
  };
  const summary = { total: 1, sent: 1, failed: [], jobs: 1 };

  function stubPrintFetch(body: unknown = t1) {
    const fn = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      void init;
      const url = typeof input === "string" ? input : input.toString();
      if (url.startsWith("/api/templates/nope")) {
        return new Response(
          JSON.stringify({ error: { code: "NotFound", message: "template not found" } }),
          { status: 404, headers: { "content-type": "application/json" } },
        );
      }
      if (url.startsWith("/api/templates/t1/source")) {
        return new Response("id: t1\n", { status: 200, headers: { "content-type": "text/yaml" } });
      }
      if (url.startsWith("/api/templates/t1")) {
        return new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });
      }
      if (url.startsWith("/api/printers")) {
        // Two printers with no default, so the one-shot preselect falls through to "none" and Print
        // stays gated on an explicit printer selection — which is what this suite exercises.
        return new Response(
          JSON.stringify([
            { id: "p1", name: "Label Printer", uri: "ipp://p1/q", insecure: false },
            { id: "p2", name: "Backup Printer", uri: "ipp://p2/q", insecure: false },
          ]),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url === "/api/settings") {
        return new Response(JSON.stringify({ default_printer_id: { value: null, is_default: true } }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.startsWith("/api/render/label")) {
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      }
      if (url === "/api/render") {
        return new Response(new Blob(["PK"]), {
          status: 200,
          headers: { "content-type": "application/zip", "content-disposition": 'attachment; filename="t1.zip"' },
        });
      }
      if (url === "/api/print") {
        return new Response(JSON.stringify(summary), { status: 200, headers: { "content-type": "application/json" } });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fn);
    return fn;
  }

  let fetchMock: ReturnType<typeof stubPrintFetch>;
  const countCalls = (path: string) =>
    fetchMock.mock.calls.filter(([u]) => String(u).startsWith(path)).length;

  function renderTemplatePage(path: string) {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const router = createMemoryRouter(
      [
        { path: "/templates/:id", element: <TemplateDetail /> },
        { path: "/", element: <div>labels grid</div> },
      ],
      { initialEntries: [path] },
    );
    return render(
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <RouterProvider router={router} />
        </ToastProvider>
      </QueryClientProvider>,
    );
  }

  beforeEach(() => {
    fetchMock = stubPrintFetch();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders a control per published parameter: textarea, number, checkbox and select", async () => {
    const templateWithParams = {
      id: "t1",
      name: "Test Template",
      description: "",
      unit: "mm",
      dpi: 300,
      params: [
        { name: "message", type: "string", control: "text", description: "Single line" },
        { name: "notes", type: "string", control: "textarea", multiline: true, default: "", description: "Notes" },
        { name: "target_width", type: "number", control: "number", default: 80, min: 25, max: 200, description: "Target width" },
        { name: "show_border", type: "boolean", control: "checkbox", default: false, description: "Show border" },
        { name: "orientation", type: "enum", control: "select", values: ["horizontal", "vertical"], default: "horizontal", description: "orientation" },
      ],
      format: { type: "single" as const, height: 18, width: { min: 25, max: 80 } },
    };

    stubPrintFetch(templateWithParams);
    renderTemplatePage("/templates/t1");
    expect(await screen.findByRole("textbox", { name: /notes/i })).toBeInstanceOf(HTMLTextAreaElement);
    expect(screen.getByRole("spinbutton", { name: /target width/i })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /show border/i })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: /orientation/i })).toBeInTheDocument();
  });

  it("gates Print on a printer, not on a filled field, then prints", async () => {
    const createUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderTemplatePage("/templates/t1");

    // The message field appears once the detail loads.
    const message = (await screen.findByLabelText("message")) as HTMLInputElement;

    // The screen does not judge completeness: Download is enabled with the field blank, and only
    // the missing printer gates Print.
    const download = screen.getByRole("button", { name: /download/i });
    const print = screen.getByRole("button", { name: /print/i });
    expect(download).toBeEnabled();
    expect(print).toBeDisabled();

    fireEvent.change(message, { target: { value: "hello" } });

    // Let the live preview settle so we can assert on the download delta.
    await waitFor(() => expect(countCalls("/api/render/label")).toBeGreaterThan(0));
    const beforeUrls = createUrl.mock.calls.length;

    // A single template downloads through /api/render: one label per copy, in the chosen format.
    const renderCall = () => [...fetchMock.mock.calls].reverse().find(([u]) => String(u) === "/api/render");
    fireEvent.click(download);
    await waitFor(() => expect(renderCall()).toBeDefined());
    await waitFor(() => expect(createUrl.mock.calls.length).toBe(beforeUrls + 1));
    const renderBody = JSON.parse((renderCall()![1] as RequestInit).body as string);
    expect(renderBody).toEqual({ template: "t1", labels: [{ data: { message: "hello" }, copies: 1 }], format: "png" });

    // Select the printer → Print enables.
    fireEvent.change(screen.getByLabelText("printer"), { target: { value: "p1" } });
    await waitFor(() => expect(print).not.toBeDisabled());

    const printCall = () => [...fetchMock.mock.calls].reverse().find(([u]) => String(u) === "/api/print");
    fireEvent.click(print);
    await waitFor(() => expect(printCall()).toBeDefined());
    const printBody = JSON.parse((printCall()![1] as RequestInit).body as string);
    expect(printBody).toEqual({ template: "t1", printer: "p1", labels: [{ data: { message: "hello" }, copies: 1 }] });
    expect(await screen.findByText("Sent 1 labels to Label Printer")).toBeInTheDocument();
  });

  it("renders the form for a template from the URL param", async () => {
    renderTemplatePage("/templates/t1");
    expect(await screen.findByLabelText("message")).toBeInTheDocument();
  });

  it("shows an error and the all-labels link for an unknown id", async () => {
    renderTemplatePage("/templates/nope");
    expect(await screen.findByText(/template not found/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /all labels/i })).toHaveAttribute("href", "/");
  });
});
