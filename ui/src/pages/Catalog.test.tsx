import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Catalog } from "./Catalog";
import { CATALOG_BASE } from "../api/catalog";
import { SHEET_ICON, SINGLE_ICON, iconGeometry } from "../setupTests";

const index = [
  {
    id: "brother_12mm",
    name: "Brother 12mm",
    description: "Continuous tape, text only",
    path: "tape/brother/brother_12mm.yaml",
    category: "tape",
    vendor: "brother",
    format: "single",
    media_width_mm: 12,
    fields: ["message"],
  },
  {
    id: "avery5163",
    name: "Avery 5163",
    description: "Shipping labels",
    path: "sheet/avery/avery5163.yaml",
    category: "sheet",
    vendor: "avery",
    format: "sheet",
    position_count: 10,
    media_width_mm: null,
    fields: ["message"],
  },
];

const YAML = "id: brother_12mm\nname: Brother 12mm\n";

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

interface StubOptions {
  installed?: string[];
  createStatus?: number;
  sourceStatus?: number;
  indexFails?: boolean;
}

let calls: { url: string; method: string; body?: string; headers: Headers }[] = [];

function stubFetch({
  installed = [],
  createStatus = 201,
  sourceStatus = 200,
  indexFails = false,
}: StubOptions = {}) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = init?.method ?? "GET";
    calls.push({ url, method, body: init?.body as string | undefined, headers: new Headers(init?.headers) });

    if (url === `${CATALOG_BASE}/index.json`) {
      return indexFails ? new Response("nope", { status: 503 }) : json(index);
    }
    if (url.startsWith(`${CATALOG_BASE}/`)) {
      return new Response(YAML, { status: 200, headers: { "content-type": "text/plain" } });
    }
    if (url === "/api/templates" && method === "GET") {
      return json({ templates: installed.map((id) => ({ id, name: id, format: { type: "single" } })) });
    }
    if (url.endsWith("/source")) {
      return sourceStatus === 200
        ? new Response("name: Edited locally\n", { status: 200 })
        : new Response("nope", { status: sourceStatus });
    }
    if (url.startsWith("/api/templates/") && method === "POST") {
      if (createStatus === 201) return json({ id: "brother_12mm", name: "Brother 12mm" }, 201);
      const code = createStatus === 409 ? "Conflict" : "TemplateInvalid";
      return json({ error: { code, message: `failed with ${createStatus}` } }, createStatus);
    }
    if (url.startsWith("/api/templates/") && method === "PUT") {
      return json({ id: "brother_12mm", name: "Brother 12mm" });
    }
    throw new Error(`unexpected fetch ${method} ${url}`);
  });
}

const templateWrites = (method: string) =>
  calls.filter((c) => c.method === method && c.url.startsWith("/api/templates/"));

function renderCatalog() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={["/templates/catalog"]}>
          <Catalog />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Catalog", () => {
  beforeEach(() => {
    calls = [];
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("lists catalog entries grouped by category and vendor", async () => {
    vi.stubGlobal("fetch", stubFetch());
    renderCatalog();
    expect(await screen.findByText("Brother 12mm")).toBeInTheDocument();
    expect(screen.getByText("Avery 5163")).toBeInTheDocument();
    expect(screen.getByLabelText("tape · brother")).toBeInTheDocument();
    expect(screen.getByLabelText("sheet · avery")).toBeInTheDocument();
  });

  // #211: Catalog now shows the same badge as the installed grid and detail pages, with the sheet
  // count from the index. Text alone would pass for a plain string or a different label, and a
  // Catalog that kept printing `entry.format` as prose would leave no `[data-format]` at all.
  it("states each entry's format with the shared badge and its sheet count", async () => {
    vi.stubGlobal("fetch", stubFetch());
    renderCatalog();
    await screen.findByText("Avery 5163");

    const sheetBadge = document.querySelector<HTMLElement>('[data-format="sheet"]')!;
    expect(sheetBadge.textContent).toBe("sheet · 10");
    expect(sheetBadge.style.color).toBe("var(--info)");
    expect(sheetBadge.style.background).toBe("var(--info-soft)");
    expect(sheetBadge.style.borderColor).toBe("var(--info)");
    expect(iconGeometry(sheetBadge)).toEqual(SHEET_ICON);

    const singleBadge = document.querySelector<HTMLElement>('[data-format="single"]')!;
    expect(singleBadge.textContent).toBe("single");
    expect(singleBadge.style.color).toBe("var(--accent)");
    expect(singleBadge.style.background).toBe("var(--accent-soft)");
    expect(singleBadge.style.borderColor).toBe("var(--accent)");
    expect(iconGeometry(singleBadge)).toEqual(SINGLE_ICON);

    // One badge per card, inside the format row — not a leftover plain string beside it.
    expect(document.querySelectorAll("[data-format]")).toHaveLength(2);
    const formatRow = sheetBadge.closest("dd")!;
    expect(formatRow).toBe(sheetBadge.parentElement);
  });

  it("marks an entry that is already installed", async () => {
    vi.stubGlobal("fetch", stubFetch({ installed: ["brother_12mm"] }));
    renderCatalog();
    expect(await screen.findByText("installed")).toBeInTheDocument();
    // and the action becomes Reinstall rather than Install
    expect(screen.getByRole("button", { name: /reinstall/i })).toBeInTheDocument();
  });

  it("installs by downloading the YAML and POSTing it, with no If-None-Match", async () => {
    vi.stubGlobal("fetch", stubFetch());
    renderCatalog();
    await screen.findByText("Brother 12mm");
    const brotherCard = screen.getByText("Brother 12mm").closest("div.rounded-lg") as HTMLElement;
    const install = within(brotherCard).getByRole("button", { name: /^install$/i });
    fireEvent.click(install);
    expect(await screen.findByText("Installed brother_12mm")).toBeInTheDocument();
    const posts = templateWrites("POST");
    expect(posts.map((c) => c.url)).toEqual(["/api/templates/brother_12mm"]);
    expect(posts[0].body).toBe(YAML);
    expect(posts[0].headers.has("if-none-match")).toBe(false);
    expect(templateWrites("PUT")).toHaveLength(0);
  });

  it("offers replace with a diff on a 409, and replaces with a PUT only when confirmed", async () => {
    vi.stubGlobal("fetch", stubFetch({ installed: ["brother_12mm"], createStatus: 409 }));
    renderCatalog();
    fireEvent.click((await screen.findAllByRole("button", { name: /reinstall/i }))[0]);
    const dialog = await screen.findByRole("dialog", { name: /replace brother_12mm/i });
    // the diff shows what is on disk next to what the catalog has
    expect(dialog).toHaveTextContent("Edited locally");
    expect(templateWrites("POST")).toHaveLength(1);
    expect(templateWrites("PUT")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: /^replace$/i }));
    expect(await screen.findByText("Replaced brother_12mm")).toBeInTheDocument();
    const puts = templateWrites("PUT");
    expect(puts.map((c) => c.url)).toEqual(["/api/templates/brother_12mm"]);
    expect(puts[0].body).toBe(YAML);
  });

  it("sends nothing when the replace dialog is cancelled", async () => {
    vi.stubGlobal("fetch", stubFetch({ installed: ["brother_12mm"], createStatus: 409 }));
    renderCatalog();
    fireEvent.click((await screen.findAllByRole("button", { name: /reinstall/i }))[0]);
    await screen.findByRole("dialog", { name: /replace brother_12mm/i });
    const before = calls.length;
    fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(calls.slice(before)).toEqual([]);
    expect(templateWrites("PUT")).toHaveLength(0);
  });

  // Regression guard: pins the existing /source-failure branch of install (the toast, no dialog),
  // which the "Catalog page" requirement now states. A dialog with a blank "Installed" side would
  // invite a Replace that overwrites a file nobody has seen.
  it("reports an unreadable installed template instead of opening the dialog (regression guard)", async () => {
    vi.stubGlobal(
      "fetch",
      stubFetch({ installed: ["brother_12mm"], createStatus: 409, sourceStatus: 404 }),
    );
    renderCatalog();
    fireEvent.click((await screen.findAllByRole("button", { name: /reinstall/i }))[0]);
    expect(await screen.findByText(/could not read the installed template/i)).toBeInTheDocument();
    expect(calls.some((c) => c.url === "/api/templates/brother_12mm/source")).toBe(true);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(templateWrites("PUT")).toHaveLength(0);
  });

  it("explains a 422 as needing a newer labeler", async () => {
    vi.stubGlobal("fetch", stubFetch({ createStatus: 422 }));
    renderCatalog();
    fireEvent.click((await screen.findAllByRole("button", { name: /^install$/i }))[0]);
    expect(await screen.findByText(/needs a newer version of labeler/i)).toBeInTheDocument();
  });

  it("says so when the catalog cannot be reached, and offers the paste page", async () => {
    vi.stubGlobal("fetch", stubFetch({ indexFails: true }));
    renderCatalog();
    expect(await screen.findByText(/couldn't reach the template catalog/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /paste a template as yaml/i })).toHaveAttribute(
      "href",
      "/templates/new",
    );
  });
});
