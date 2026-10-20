import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Templates } from "./Templates";
import { readCategories } from "../lib/categorySource";
import { SHEET_ICON, SINGLE_ICON, iconGeometry } from "../setupTests";

const templates: Array<{
  id: string;
  name: string;
  description: string;
  unit: string;
  dpi: number;
  format: Record<string, unknown>;
  categories: string[];
}> = [
  {
    id: "brother_24mm_qr",
    name: "Brother 24mm",
    description: "Continuous label roll",
    unit: "mm",
    dpi: 300,
    format: { type: "single", width: 80, height: 24 },
    categories: [],
  },
  {
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
    categories: [],
  },
];

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

type Broken = { path: string; error: string };

// Route the fetch mock by URL: /api/templates returns the template list, while /api/favorites and
// /api/recent-templates default to [] (so their rows stay hidden). Favorites is a mutable closure so a
// PUT/DELETE to /api/favorites/{id} updates what the next refetch returns.
function stubFetch(opts?: {
  favorites?: string[];
  recent?: string[];
  empty?: boolean;
  templates?: typeof templates;
  sources?: Record<string, string>;
  putFails?: (id: string) => boolean;
  holdPut?: Promise<void>;
  broken?: Broken[];
  listGate?: Promise<void>;
  reloadGate?: Promise<void>;
  reloadResult?: { templates?: typeof templates; broken?: Broken[] };
  reloadError?: { status: number; body: unknown };
}) {
  let favorites = [...(opts?.favorites ?? [])];
  const recent = [...(opts?.recent ?? [])];
  let currentTemplates = [...(opts?.templates ?? (opts?.empty ? [] : templates))];
  let currentBroken = [...(opts?.broken ?? [])];
  const sources = { ...(opts?.sources ?? {}) };
  const calls: { method: string; url: string }[] = [];
  const writes: { id: string; body: string }[] = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : (input as Request).url;
    const method = init?.method ?? "GET";
    calls.push({ method, url });

    if (url.startsWith("/api/favorites/")) {
      const id = decodeURIComponent(url.slice("/api/favorites/".length));
      if (method === "PUT" && !favorites.includes(id)) favorites = [...favorites, id];
      if (method === "DELETE") favorites = favorites.filter((f) => f !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/templates/") && url.endsWith("/source")) {
      const id = decodeURIComponent(url.slice("/api/templates/".length, -"/source".length));
      return new Response(sources[id] ?? "", { status: 200, headers: { "content-type": "text/plain" } });
    }
    if (url.startsWith("/api/templates/") && method === "PUT") {
      const id = decodeURIComponent(url.slice("/api/templates/".length));
      const body = typeof init?.body === "string" ? init.body : "";
      writes.push({ id, body });
      if (opts?.holdPut) await opts.holdPut;
      if (opts?.putFails?.(id)) {
        return new Response(
          JSON.stringify({ error: { code: "Invalid", message: "server said no" } }),
          { status: 422, headers: { "content-type": "application/json" } },
        );
      }
      sources[id] = body;
      const parsed = readCategories(body);
      if (parsed.ok) {
        const index = currentTemplates.findIndex((t) => t.id === id);
        if (index >= 0) {
          currentTemplates[index] = { ...currentTemplates[index], categories: parsed.categories };
        }
      }
      const updated = currentTemplates.find((t) => t.id === id);
      return jsonResponse({ ...(updated ?? { id, name: id, description: "", unit: "mm", dpi: 300, format: {} }), variables: [] });
    }
    if (url === "/api/favorites") return jsonResponse(favorites);
    if (url === "/api/recent-templates") return jsonResponse(recent);
    if (url === "/api/template-registry/reload") {
      if (opts?.reloadGate) await opts.reloadGate;
      if (opts?.reloadError) return jsonResponse(opts.reloadError.body, opts.reloadError.status);
      if (opts?.reloadResult) {
        if (opts.reloadResult.templates) currentTemplates = [...opts.reloadResult.templates];
        if (opts.reloadResult.broken) currentBroken = [...opts.reloadResult.broken];
      }
      return jsonResponse({ count: currentTemplates.length, broken_count: currentBroken.length });
    }
    if (url === "/api/templates") {
      if (opts?.listGate) await opts.listGate;
      return jsonResponse({
        templates: currentTemplates,
        ...(currentBroken.length ? { broken: currentBroken } : {}),
      });
    }
    return jsonResponse({
      templates: currentTemplates,
      ...(currentBroken.length ? { broken: currentBroken } : {}),
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  return { calls, writes };
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter>
          <Templates />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Templates list", () => {
  beforeEach(() => {
    stubFetch();
  });

  it("renders both names and their format badges", async () => {
    renderPage();
    expect(await screen.findByText("Brother 24mm")).toBeInTheDocument();
    expect(screen.getByText("Avery 5163")).toBeInTheDocument();
    expect(screen.getByText("single")).toBeInTheDocument();
    expect(screen.getByText("sheet · 6")).toBeInTheDocument();
  });

  // The grid and the detail page must render the same badge (#201). Text and a rect count alone
  // would pass for two six-cell icons of different geometry, or for a pill wearing the wrong
  // colours, so the geometry and the colour tokens are compared too. TemplateDetail.test.tsx
  // asserts the same four things against the same shape.
  it("renders the sheet badge with its icon geometry and its own colour tokens", async () => {
    renderPage();
    await screen.findByText("Avery 5163");
    const badge = document.querySelector<HTMLElement>('[data-format="sheet"]')!;
    expect(badge.textContent).toBe("sheet · 6");
    expect(badge.style.color).toBe("var(--info)");
    expect(badge.style.background).toBe("var(--info-soft)");
    expect(badge.style.borderColor).toBe("var(--info)");
    expect(iconGeometry(badge)).toEqual(SHEET_ICON);
  });

  it("renders the single badge with its icon and its own colour tokens", async () => {
    renderPage();
    await screen.findByText("Brother 24mm");
    const badge = document.querySelector<HTMLElement>('[data-format="single"]')!;
    expect(badge.textContent).toBe("single");
    expect(badge.style.color).toBe("var(--accent)");
    expect(badge.style.background).toBe("var(--accent-soft)");
    expect(badge.style.borderColor).toBe("var(--accent)");
    expect(iconGeometry(badge)).toEqual(SINGLE_ICON);
    expect(badge.closest("div.rounded-lg")!.querySelectorAll("[data-format]")).toHaveLength(1);
  });

  // The badge rides the top rail. Nothing else pins that: moving it back beside the id chip would
  // collapse the id to a single character again and no other assertion would notice.
  it("puts the badge on the top rail, above the thumbnail and the id chip", async () => {
    renderPage();
    await screen.findByText("Avery 5163");
    const badge = document.querySelector<HTMLElement>('[data-format="sheet"]')!;
    const rail = badge.parentElement!;
    // not beside the id chip it used to squeeze
    expect(rail.querySelector("code")).toBeNull();
    // Document order pins the rail to the top of the card.
    const card = badge.closest("div.rounded-lg")!;
    // Against the card's main link, which wraps both the thumbnail and the title: preceding the
    // title alone would still allow the rail to sit under the thumbnail. Not against a
    // thumbnail-ish selector, which the badge's own aria-hidden icon would match first.
    const mainLink = card.querySelector('a[aria-label^="Print "]')!;
    expect(badge.compareDocumentPosition(mainLink) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(badge.compareDocumentPosition(card.querySelector("code")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Exactly one badge on the card: a legacy pill left behind beside the new one would satisfy every
    // other assertion here, and the spec says each surface renders one badge.
    expect(card.querySelectorAll("[data-format]")).toHaveLength(1);
  });

  it("card main link goes to the template page, with no separate details link", async () => {
    renderPage();
    const card = await screen.findByRole("link", { name: "Print Brother 24mm" });
    expect(card).toHaveAttribute("href", "/templates/brother_24mm_qr");
    expect(screen.queryByRole("link", { name: "Brother 24mm template details" })).not.toBeInTheDocument();
  });

  it("filters cards by name from the search box", async () => {
    renderPage();
    await screen.findByRole("link", { name: "Print Brother 24mm" });
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "avery" } });
    expect(screen.queryByRole("link", { name: "Print Brother 24mm" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Print Avery 5163" })).toBeInTheDocument();
  });

  it("shows the Labels heading", async () => {
    renderPage();
    expect(await screen.findByRole("heading", { name: "Labels" })).toBeInTheDocument();
  });

  /// The catalog used to be reachable only from the empty-state card, which vanishes once any
  /// template exists — so on a populated install it could only be reached by typing the URL.
  it("offers a permanent way into the catalog, not just from the empty state", async () => {
    renderPage();
    const link = await screen.findByRole("link", { name: /browse catalog/i });
    expect(link).toHaveAttribute("href", "/templates/catalog");
  });

  it("filters cards by id from the search box", async () => {
    renderPage();
    await screen.findByText("Brother 24mm");
    const search = screen.getByRole("searchbox");
    fireEvent.change(search, { target: { value: "avery" } });
    expect(screen.getByText("Avery 5163")).toBeInTheDocument();
    expect(screen.queryByText("Brother 24mm")).not.toBeInTheDocument();
  });

  it("renders a thumbnail image per card pointing at the thumbnail endpoint", async () => {
    renderPage();
    const img = await screen.findByAltText("Brother 24mm preview");
    expect(img).toHaveAttribute("src", "/api/templates/brother_24mm_qr/thumbnail");
    expect(img.tagName).toBe("IMG");
  });

  it("falls back to a placeholder when the thumbnail image fails to load", async () => {
    renderPage();
    const img = await screen.findByAltText("Avery 5163 preview");
    fireEvent.error(img);
    expect(screen.getByText("preview", { selector: "div" })).toBeInTheDocument();
  });

  it("shows Favorites and Recent rows only when non-empty, deduped", async () => {
    stubFetch({ favorites: ["brother_24mm_qr"], recent: ["brother_24mm_qr", "avery5163"] });
    renderPage();
    const favRegion = await screen.findByRole("region", { name: "Favorites" });
    // Favorites row shows Brother only.
    expect(within(favRegion).getByRole("link", { name: "Print Brother 24mm" })).toBeInTheDocument();
    expect(
      within(favRegion).queryByRole("link", { name: "Print Avery 5163" }),
    ).not.toBeInTheDocument();
    // Recent row excludes the favorited Brother (dedupe), leaving only Avery.
    const recentRegion = screen.getByRole("region", { name: "Recent" });
    expect(within(recentRegion).getByRole("link", { name: "Print Avery 5163" })).toBeInTheDocument();
    expect(
      within(recentRegion).queryByRole("link", { name: "Print Brother 24mm" }),
    ).not.toBeInTheDocument();
  });

  it("hides the rows while searching", async () => {
    stubFetch({ favorites: ["brother_24mm_qr"] });
    renderPage();
    await screen.findByRole("region", { name: "Favorites" });
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "avery" } });
    expect(screen.queryByRole("region", { name: "Favorites" })).not.toBeInTheDocument();
  });

  it("star toggle favorites and unfavorites", async () => {
    const { calls } = stubFetch();
    renderPage();
    // Rows start empty; the grid card exposes a "favorite" star.
    const favBtn = await screen.findByRole("button", { name: "favorite Brother 24mm" });
    fireEvent.click(favBtn);
    await waitFor(() =>
      expect(
        calls.some((c) => c.method === "PUT" && c.url === "/api/favorites/brother_24mm_qr"),
      ).toBe(true),
    );
    // After invalidation the Favorites row appears; its star now toggles the other way.
    const favRegion = await screen.findByRole("region", { name: "Favorites" });
    const unfavBtn = await within(favRegion).findByRole("button", {
      name: "unfavorite Brother 24mm",
    });
    fireEvent.click(unfavBtn);
    await waitFor(() =>
      expect(
        calls.some((c) => c.method === "DELETE" && c.url === "/api/favorites/brother_24mm_qr"),
      ).toBe(true),
    );
  });

  it("shows the first-run empty state, not a bare sentence, when nothing is installed", async () => {
    stubFetch({ empty: true });
    renderPage();
    expect(await screen.findByText(/no templates yet/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /browse the catalog/i })).toHaveAttribute(
      "href",
      "/templates/catalog",
    );
    expect(screen.getByRole("link", { name: /paste yaml/i })).toHaveAttribute(
      "href",
      "/templates/new",
    );
  });

  it("keeps the search-miss message distinct from having nothing installed", async () => {
    stubFetch();
    renderPage();
    await screen.findByText("Brother 24mm");
    fireEvent.change(screen.getByLabelText(/search templates/i), {
      target: { value: "zzzz-no-match" },
    });
    expect(await screen.findByText("No templates match.")).toBeInTheDocument();
    expect(screen.queryByText(/no templates yet/i)).toBeNull();
  });

  const tpl = (id: string, name: string, categories: string[]) => ({
    id,
    name,
    description: "",
    unit: "mm",
    dpi: 300,
    format: { type: "single" as const, width: 50, height: 25 },
    categories,
  });

  it("filters by category: a template shows under each category it lists", async () => {
    stubFetch({
      templates: [
        tpl("pallet", "Pallet", ["Shipping", "Warehouse"]),
        tpl("crate", "Crate", ["Archive"]),
        tpl("bin", "Bin", []),
      ],
    });
    renderPage();
    await screen.findByText("Pallet");

    fireEvent.click(screen.getByRole("button", { name: "Shipping" }));
    expect(screen.getByText("Pallet")).toBeInTheDocument();
    expect(screen.queryByText("Crate")).not.toBeInTheDocument();
    expect(screen.queryByText("Bin")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Warehouse" }));
    expect(screen.getByText("Pallet")).toBeInTheDocument();
    expect(screen.queryByText("Crate")).not.toBeInTheDocument();
    expect(screen.queryByText("Bin")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Uncategorized" }));
    expect(screen.getByText("Bin")).toBeInTheDocument();
    expect(screen.queryByText("Pallet")).not.toBeInTheDocument();
    expect(screen.queryByText("Crate")).not.toBeInTheDocument();

    // Chips are sorted, not in the order the templates first mention them.
    const toolbar = screen.getByRole("toolbar", { name: "Category filter" });
    expect(within(toolbar).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "All",
      "Archive",
      "Shipping",
      "Warehouse",
      "Uncategorized",
    ]);

    // Composes with search; a filtered miss says nothing matches, not the first-run empty state.
    fireEvent.click(within(toolbar).getByRole("button", { name: "Shipping" }));
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "bin" } });
    expect(screen.getByText("No templates match.")).toBeInTheDocument();
    expect(screen.queryByText("Pallet")).not.toBeInTheDocument();
    expect(screen.queryByText(/no templates yet/i)).toBeNull();
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "pal" } });
    expect(screen.getByText("Pallet")).toBeInTheDocument();
  });

  it("treats categories literally named All and Uncategorized as categories, not as the filters", async () => {
    stubFetch({
      templates: [
        tpl("named_all", "Named All", ["All"]),
        tpl("named_uncat", "Named Uncategorized", ["Uncategorized"]),
        tpl("loose", "Loose", []),
      ],
    });
    renderPage();
    await screen.findByText("Loose");
    const toolbar = screen.getByRole("toolbar", { name: "Category filter" });
    const [allFilter, allCategory] = within(toolbar).getAllByRole("button", { name: "All" });
    const [uncatCategory, uncatFilter] = within(toolbar).getAllByRole("button", { name: "Uncategorized" });

    fireEvent.click(allCategory);
    expect(screen.getByText("Named All")).toBeInTheDocument();
    expect(screen.queryByText("Named Uncategorized")).not.toBeInTheDocument();
    expect(screen.queryByText("Loose")).not.toBeInTheDocument();

    fireEvent.click(uncatCategory);
    expect(screen.getByText("Named Uncategorized")).toBeInTheDocument();
    expect(screen.queryByText("Named All")).not.toBeInTheDocument();
    expect(screen.queryByText("Loose")).not.toBeInTheDocument();

    fireEvent.click(uncatFilter);
    expect(screen.getByText("Loose")).toBeInTheDocument();
    expect(screen.queryByText("Named Uncategorized")).not.toBeInTheDocument();

    fireEvent.click(allFilter);
    expect(screen.getByText("Loose")).toBeInTheDocument();
    expect(screen.getByText("Named All")).toBeInTheDocument();
    expect(screen.getByText("Named Uncategorized")).toBeInTheDocument();
  });

  it("omits the Uncategorized filter when every template has a category", async () => {
    stubFetch({ templates: [tpl("t1", "T1", ["A"]), tpl("t2", "T2", ["B"])] });
    renderPage();
    await screen.findByText("T1");
    const toolbar = screen.getByRole("toolbar", { name: "Category filter" });
    expect(within(toolbar).getByRole("button", { name: "A" })).toBeInTheDocument();
    expect(within(toolbar).queryByRole("button", { name: "Uncategorized" })).not.toBeInTheDocument();
  });

  it("hides Favorites and Recents under a category filter and restores them on All", async () => {
    stubFetch({
      templates: [tpl("t1", "T1", ["Warehouse"]), tpl("t2", "T2", ["Shipping"])],
      favorites: ["t1"],
      recent: ["t2"],
    });
    renderPage();

    expect(await screen.findByRole("region", { name: "Favorites" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Recent" })).toBeInTheDocument();

    const toolbar = screen.getByRole("toolbar", { name: "Category filter" });
    fireEvent.click(within(toolbar).getByRole("button", { name: "Warehouse" }));
    expect(screen.queryByRole("region", { name: "Favorites" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Recent" })).not.toBeInTheDocument();

    fireEvent.click(within(toolbar).getByRole("button", { name: "All" }));
    expect(screen.getByRole("region", { name: "Favorites" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Recent" })).toBeInTheDocument();
  });

  const editButton = (name: string) =>
    screen.getByRole("button", { name: `Edit categories for ${name}` });

  const filterButton = (name: string) =>
    within(screen.getByRole("toolbar", { name: "Category filter" })).getByRole("button", { name });

  it("saves edited categories, refreshes the summaries and returns a removed filter to All", async () => {
    const { writes } = stubFetch({
      templates: [tpl("pallet", "Pallet", ["Shipping"])],
      sources: { pallet: "id: pallet\nname: Pallet\ncategories: [Shipping]\n" },
    });
    renderPage();
    await screen.findByText("Pallet");
    fireEvent.click(filterButton("Shipping"));

    fireEvent.click(editButton("Pallet"));
    const input = await screen.findByLabelText("Category 1");
    expect(input).toHaveValue("Shipping");
    fireEvent.change(input, { target: { value: "Warehouse" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toEqual({
      id: "pallet",
      body: 'id: pallet\nname: Pallet\ncategories: ["Warehouse"]\n',
    });
    expect(await screen.findByText("Saved pallet")).toBeInTheDocument();

    await waitFor(() => {
      const toolbar = screen.getByRole("toolbar", { name: "Category filter" });
      expect(within(toolbar).queryByRole("button", { name: "Shipping" })).not.toBeInTheDocument();
      expect(within(toolbar).getByRole("button", { name: "Warehouse" })).toBeInTheDocument();
      expect(within(toolbar).getByRole("button", { name: "All" })).toHaveAttribute("aria-pressed", "true");
    });
    fireEvent.click(filterButton("Warehouse"));
    expect(screen.getByText("Pallet")).toBeInTheDocument();
  });

  it("inserts an absent categories key when saving", async () => {
    const { writes } = stubFetch({
      templates: [tpl("loose", "Loose", [])],
      sources: { loose: "id: loose\nname: Loose\n" },
    });
    renderPage();
    await screen.findByText("Loose");

    fireEvent.click(editButton("Loose"));
    expect(await screen.findByText("Edit categories for loose")).toBeInTheDocument();
    expect(screen.queryByLabelText("Category 1")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.change(screen.getByLabelText("Category 1"), { target: { value: "Shipping" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.change(screen.getByLabelText("Category 2"), { target: { value: "Warehouse" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toEqual({
      id: "loose",
      body: 'categories: ["Shipping","Warehouse"]\nid: loose\nname: Loose\n',
    });
  });

  it("clears categories to an empty list", async () => {
    const { writes } = stubFetch({
      templates: [tpl("pallet", "Pallet", ["Shipping"])],
      sources: { pallet: "id: pallet\ncategories: [Shipping]\nname: x\n" },
    });
    renderPage();
    await screen.findByText("Pallet");

    fireEvent.click(editButton("Pallet"));
    fireEvent.click(await screen.findByRole("button", { name: "Remove category 1" }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toEqual({ id: "pallet", body: "id: pallet\ncategories: []\nname: x\n" });
  });

  it("keeps comma-bearing category strings as separate entries", async () => {
    stubFetch({
      templates: [tpl("pallet", "Pallet", ["A,B", "C"])],
      sources: { pallet: 'id: pallet\ncategories: ["A,B", "C"]\nname: x\n' },
    });
    renderPage();
    await screen.findByText("Pallet");

    fireEvent.click(editButton("Pallet"));
    const inputs = await screen.findAllByLabelText(/^Category \d+$/);
    expect(inputs).toHaveLength(2);
    expect(inputs[0]).toHaveValue("A,B");
    expect(inputs[1]).toHaveValue("C");
  });

  it("does not write or report success on Cancel", async () => {
    const { writes } = stubFetch({
      templates: [tpl("pallet", "Pallet", ["Shipping"])],
      sources: { pallet: "id: pallet\ncategories: [Shipping]\nname: x\n" },
    });
    renderPage();
    await screen.findByText("Pallet");

    fireEvent.click(editButton("Pallet"));
    fireEvent.change(await screen.findByLabelText("Category 1"), { target: { value: "Warehouse" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() =>
      expect(screen.queryByText("Edit categories for pallet")).not.toBeInTheDocument(),
    );
    expect(writes).toHaveLength(0);
    expect(screen.queryByText("Saved pallet")).not.toBeInTheDocument();
  });

  it("returns a removed filter to All when the save settles after the editor is cancelled", async () => {
    let release!: () => void;
    const holdPut = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { writes } = stubFetch({
      templates: [tpl("pallet", "Pallet", ["Shipping"])],
      sources: { pallet: "id: pallet\ncategories: [Shipping]\nname: x\n" },
      holdPut,
    });
    renderPage();
    await screen.findByText("Pallet");
    fireEvent.click(filterButton("Shipping"));

    fireEvent.click(editButton("Pallet"));
    fireEvent.change(await screen.findByLabelText("Category 1"), { target: { value: "Warehouse" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(writes).toHaveLength(1));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByText("Edit categories for pallet")).not.toBeInTheDocument(),
    );

    release();
    await waitFor(() => {
      const toolbar = screen.getByRole("toolbar", { name: "Category filter" });
      expect(within(toolbar).queryByRole("button", { name: "Shipping" })).not.toBeInTheDocument();
      expect(within(toolbar).getByRole("button", { name: "Warehouse" })).toBeInTheDocument();
      expect(within(toolbar).getByRole("button", { name: "All" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
    });
    expect(screen.getByText("Pallet")).toBeInTheDocument();
  });

  it("keeps a reopened editor's draft when an earlier save settles", async () => {
    let release!: () => void;
    const holdPut = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { writes } = stubFetch({
      templates: [tpl("pallet", "Pallet", ["Shipping"])],
      sources: { pallet: "id: pallet\ncategories: [Shipping]\nname: x\n" },
      holdPut,
    });
    renderPage();
    await screen.findByText("Pallet");

    fireEvent.click(editButton("Pallet"));
    fireEvent.change(await screen.findByLabelText("Category 1"), { target: { value: "Warehouse" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(writes).toHaveLength(1));

    // Cancel the saving editor and reopen the same template with a fresh draft.
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByText("Edit categories for pallet")).not.toBeInTheDocument(),
    );
    fireEvent.click(editButton("Pallet"));
    fireEvent.click(await screen.findByRole("button", { name: "Add" }));
    fireEvent.change(screen.getByLabelText("Category 2"), { target: { value: "Archive" } });

    release();

    // The completed save must not close the reopened editor or discard its new entry.
    expect(await screen.findByText("Saved pallet")).toBeInTheDocument();
    expect(screen.getByText("Edit categories for pallet")).toBeInTheDocument();
    expect(screen.getByLabelText("Category 2")).toHaveValue("Archive");
  });

  it("refuses an anchored categories list and links to the YAML editor without writing", async () => {
    const { writes } = stubFetch({
      templates: [tpl("pallet", "Pallet", ["Old"])],
      sources: { pallet: "base: &c [Old]\ncategories: *c\n" },
    });
    renderPage();
    await screen.findByText("Pallet");

    fireEvent.click(editButton("Pallet"));
    const link = await screen.findByRole("link", { name: "template YAML editor" });
    expect(link).toHaveAttribute("href", "/templates/pallet");
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    expect(writes).toHaveLength(0);
  });

  it("shows a failed save without reporting success and keeps the edit", async () => {
    const { writes } = stubFetch({
      templates: [tpl("pallet", "Pallet", ["Shipping"])],
      sources: { pallet: "id: pallet\ncategories: [Shipping]\nname: x\n" },
      putFails: (id) => id === "pallet",
    });
    renderPage();
    await screen.findByText("Pallet");

    fireEvent.click(editButton("Pallet"));
    fireEvent.change(await screen.findByLabelText("Category 1"), { target: { value: "Warehouse" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(writes).toHaveLength(1));
    expect((await screen.findAllByText("server said no")).length).toBeGreaterThan(0);
    expect(screen.queryByText("Saved pallet")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Category 1")).toHaveValue("Warehouse");
  });
});

describe("Templates refused entries", () => {
  const refusedTpl = (id: string, name: string, categories: string[] = []) => ({
    id,
    name,
    description: "",
    unit: "mm",
    dpi: 300,
    format: { type: "single" as const, width: 50, height: 25 },
    categories,
  });

  it("shows the refused count while collapsed, full errors when expanded, and keeps both under filters", async () => {
    stubFetch({
      templates: [refusedTpl("pallet", "Pallet", ["Shipping"])],
      broken: [
        { path: "bad.yaml", error: "bad.yaml: unknown field `ink`" },
        { path: "notes.txt", error: "notes.txt: not a template file" },
      ],
    });
    renderPage();
    await screen.findByText("Pallet");
    expect(screen.getByText("Refused entries (2)")).toBeInTheDocument();
    expect(screen.getByText("bad.yaml: unknown field `ink`")).not.toBeVisible();

    fireEvent.click(screen.getByText("Refused entries (2)"));
    expect(await screen.findByText("bad.yaml: unknown field `ink`")).toBeVisible();
    expect(screen.getByText("bad.yaml")).toBeVisible();
    expect(screen.getByText("notes.txt")).toBeVisible();
    expect(screen.getByText("notes.txt: not a template file")).toBeVisible();

    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "zzz-no-match" } });
    expect(screen.getByText("No templates match.")).toBeInTheDocument();
    expect(screen.getByText("Refused entries (2)")).toBeInTheDocument();
    expect(screen.getByText("notes.txt: not a template file")).toBeInTheDocument();
  });

  it("distinguishes an all-refused registry from an empty installation", async () => {
    stubFetch({ empty: true, broken: [{ path: "broken.yaml", error: "broken.yaml: bad layout" }] });
    renderPage();
    expect(await screen.findByText("Refused entries (1)")).toBeInTheDocument();
    expect(screen.getByText("No printable templates.")).toBeInTheDocument();
    expect(screen.getByText("broken.yaml: bad layout")).not.toBeVisible();
    expect(screen.queryByText(/no templates yet/i)).toBeNull();

    fireEvent.click(screen.getByText("Refused entries (1)"));
    expect(await screen.findByText("broken.yaml: bad layout")).toBeVisible();
  });

  it("offers Reload templates even when nothing is refused", async () => {
    stubFetch();
    renderPage();
    await screen.findByText("Brother 24mm");
    expect(await screen.findByRole("button", { name: "Reload templates" })).toBeInTheDocument();
    expect(screen.queryByText(/refused entries/i)).toBeNull();
  });

  it("awaits the registry reload before refreshing, turning a repaired refusal printable", async () => {
    const listGate = deferred<void>();
    const gate = deferred<void>();
    const { calls } = stubFetch({
      empty: true,
      broken: [{ path: "bad.yaml", error: "bad.yaml: bad layout" }],
      listGate: listGate.promise,
      reloadGate: gate.promise,
      reloadResult: { templates: [refusedTpl("fixed", "Fixed")], broken: [] },
    });
    renderPage();
    const reload = await screen.findByRole("button", { name: "Reload templates" });
    // While the initial listing is still in flight, activating reload would let that stale response
    // land after the reload POST and be reused by invalidateQueries instead of a fresh GET.
    expect(reload).toBeDisabled();
    listGate.resolve();
    await screen.findByText("Refused entries (1)");
    expect(reload).toBeEnabled();
    const listGets = () => calls.filter((c) => c.method === "GET" && c.url === "/api/templates").length;
    const baseline = listGets();

    fireEvent.click(reload);
    await waitFor(() =>
      expect(
        calls.some((c) => c.method === "POST" && c.url === "/api/template-registry/reload"),
      ).toBe(true),
    );
    // The POST is still gated, so no list refresh may have started.
    expect(listGets()).toBe(baseline);
    expect(reload).toBeDisabled();

    gate.resolve();
    expect(await screen.findByText("Fixed")).toBeInTheDocument();
    expect(screen.queryByText(/refused entries/i)).toBeNull();
    expect(screen.queryByText("No printable templates.")).toBeNull();
    expect(listGets()).toBeGreaterThan(baseline);
  });

  it("keeps refusals that survive a successful reload", async () => {
    stubFetch({
      templates: [refusedTpl("good", "Good")],
      broken: [{ path: "bad.yaml", error: "bad.yaml: bad layout" }],
      reloadResult: {
        templates: [refusedTpl("good", "Good")],
        broken: [{ path: "notes.txt", error: "notes.txt: not a template file" }],
      },
    });
    renderPage();
    fireEvent.click(await screen.findByText("Refused entries (1)"));
    expect(await screen.findByText("bad.yaml: bad layout")).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Reload templates" }));
    expect(await screen.findByText("notes.txt: not a template file")).toBeVisible();
    expect(screen.queryByText("bad.yaml: bad layout")).toBeNull();
    expect(screen.getByText("Refused entries (1)")).toBeInTheDocument();
  });

  it("reports a failed reload and keeps the displayed templates and refusals", async () => {
    stubFetch({
      templates: [refusedTpl("good", "Good")],
      broken: [{ path: "bad.yaml", error: "bad.yaml: bad layout" }],
      reloadError: {
        status: 500,
        body: { error: { code: "Internal", message: "registry read failed" } },
      },
    });
    renderPage();
    await screen.findByText("Good");
    expect(screen.getByText("Refused entries (1)")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Reload templates" }));
    expect(await screen.findByText("registry read failed")).toBeInTheDocument();
    expect(screen.getByText("Good")).toBeInTheDocument();
    expect(screen.getByText("Refused entries (1)")).toBeInTheDocument();
  });
});
