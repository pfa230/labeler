import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Templates } from "./Templates";
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

function jsonResponse(body: unknown) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

// Route the fetch mock by URL: /api/templates returns the template list, while /api/favorites and
// /api/recent-templates default to [] (so their rows stay hidden). Favorites is a mutable closure so a
// PUT/DELETE to /api/favorites/{id} updates what the next refetch returns.
function stubFetch(opts?: {
  favorites?: string[];
  recent?: string[];
  empty?: boolean;
  templates?: typeof templates;
}) {
  let favorites = [...(opts?.favorites ?? [])];
  const recent = [...(opts?.recent ?? [])];
  const currentTemplates = [...(opts?.templates ?? (opts?.empty ? [] : templates))];
  const calls: { method: string; url: string }[] = [];
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
    if (url === "/api/favorites") return jsonResponse(favorites);
    if (url === "/api/recent-templates") return jsonResponse(recent);
    return jsonResponse({ templates: currentTemplates });
  });
  vi.stubGlobal("fetch", fetchMock);
  return calls;
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

  it("card main link goes to the print form; details link to the template page", async () => {
    renderPage();
    // The card link gets aria-label "Print {name}" so queries are unambiguous vs the details link
    // (a bare /brother 24mm/i regex would match BOTH links' accessible names).
    const card = await screen.findByRole("link", { name: "Print Brother 24mm" });
    expect(card).toHaveAttribute("href", "/print/brother_24mm_qr");
    const details = screen.getByRole("link", { name: "Brother 24mm template details" });
    expect(details).toHaveAttribute("href", "/templates/brother_24mm_qr");
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
    const calls = stubFetch();
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
});
