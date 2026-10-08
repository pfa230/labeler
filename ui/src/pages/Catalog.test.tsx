import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Catalog } from "./Catalog";
import { CATALOG_BASE } from "../api/catalog";
import { noBadgeStyling } from "../setupTests";

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

  // #201 gave installed templates a format badge and deliberately left the catalog alone: a catalog
  // entry is not installed yet and CatalogEntry.format is a bare string with no positions, so a badge
  // here could carry an icon and a colour but never the count that makes the badge worth having.
  it("states a catalog entry's format as plain prose, with no badge", async () => {
    vi.stubGlobal("fetch", stubFetch());
    renderCatalog();
    await screen.findByText("Avery 5163");
    const format = screen.getByText("sheet", { selector: "dd" });
    expect(format.textContent).toBe("sheet");
    expect(format).not.toHaveAttribute("data-format");
    expect(format.querySelector("svg")).toBeNull();
    expect(format.querySelector("[data-format]")).toBeNull();
    expect(noBadgeStyling(format)).toBe(true);
    expect(document.querySelector("[data-format]")).toBeNull();
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
