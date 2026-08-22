import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Templates } from "./Templates";

const templates = [
  {
    id: "brother_24mm_qr",
    name: "Brother 24mm",
    description: "Continuous label roll",
    unit: "mm",
    dpi: 300,
    format: { type: "single", width: 80, height: 24 },
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
      positions: [[0, 0]],
    },
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
  failMoveIds?: string[];
}) {
  let favorites = [...(opts?.favorites ?? [])];
  const recent = [...(opts?.recent ?? [])];
  let currentTemplates = [...(opts?.templates ?? (opts?.empty ? [] : templates))];
  const calls: { method: string; url: string; body?: unknown }[] = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : (input as Request).url;
    const method = init?.method ?? "GET";
    const bodyText = typeof init?.body === "string" ? init.body : undefined;
    const parsedBody = bodyText ? JSON.parse(bodyText) : undefined;
    calls.push({ method, url, body: parsedBody });

    if (url.startsWith("/api/favorites/")) {
      const id = decodeURIComponent(url.slice("/api/favorites/".length));
      if (method === "PUT" && !favorites.includes(id)) favorites = [...favorites, id];
      if (method === "DELETE") favorites = favorites.filter((f) => f !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/templates/") && url.endsWith("/group") && method === "PUT") {
      const id = decodeURIComponent(url.slice("/api/templates/".length, url.length - "/group".length));
      if (opts?.failMoveIds?.includes(id)) {
        return new Response(
          JSON.stringify({
            error: { code: "TemplateGroupInvalid", message: `Cannot move ${id}` },
          }),
          { status: 422, headers: { "content-type": "application/json" } },
        );
      }
      const newGroup = parsedBody?.group ?? undefined;
      currentTemplates = currentTemplates.map((t) =>
        t.id === id ? { ...t, group: newGroup } : t,
      );
      const found = currentTemplates.find((t) => t.id === id);
      return jsonResponse(found ?? { id, group: newGroup });
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
    expect(screen.getByText("sheet")).toBeInTheDocument();
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
    expect(await screen.findByText(/no templates match your search/i)).toBeInTheDocument();
    expect(screen.queryByText(/no templates yet/i)).toBeNull();
  });

  it("filters templates by group and composes with search", async () => {
    const customTemplates = [
      {
        id: "wh_box",
        name: "Warehouse Box",
        group: "Warehouse",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
      {
        id: "wh_pallet",
        name: "Warehouse Pallet",
        group: "Warehouse",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 100, height: 50 },
      },
      {
        id: "ship_label",
        name: "Shipping Envelope",
        group: "Shipping",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 80, height: 40 },
      },
      {
        id: "ungrouped_tpl",
        name: "Ungrouped Item",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 30, height: 10 },
      },
    ];
    stubFetch({ templates: customTemplates });
    renderPage();

    expect(await screen.findByText("Warehouse Box")).toBeInTheDocument();
    expect(screen.getByText("Shipping Envelope")).toBeInTheDocument();
    expect(screen.getByText("Ungrouped Item")).toBeInTheDocument();

    // Group buttons: All, Shipping, Warehouse, Ungrouped
    const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
    expect(within(groupToolbar).getByRole("button", { name: "All" })).toBeInTheDocument();
    expect(within(groupToolbar).getByRole("button", { name: "Shipping" })).toBeInTheDocument();
    expect(within(groupToolbar).getByRole("button", { name: "Warehouse" })).toBeInTheDocument();
    expect(within(groupToolbar).getByRole("button", { name: "Ungrouped" })).toBeInTheDocument();

    // 1. Filter by Warehouse
    fireEvent.click(within(groupToolbar).getByRole("button", { name: "Warehouse" }));
    expect(screen.getByText("Warehouse Box")).toBeInTheDocument();
    expect(screen.getByText("Warehouse Pallet")).toBeInTheDocument();
    expect(screen.queryByText("Shipping Envelope")).not.toBeInTheDocument();
    expect(screen.queryByText("Ungrouped Item")).not.toBeInTheDocument();

    // 2. Compose with search
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "box" } });
    expect(screen.getByText("Warehouse Box")).toBeInTheDocument();
    expect(screen.queryByText("Warehouse Pallet")).not.toBeInTheDocument();

    // 3. Search miss within group
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "envelope" } });
    expect(screen.getByText("No templates match your search.")).toBeInTheDocument();

    // 4. Filter by Ungrouped
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "" } });
    fireEvent.click(within(groupToolbar).getByRole("button", { name: "Ungrouped" }));
    expect(screen.getByText("Ungrouped Item")).toBeInTheDocument();
    expect(screen.queryByText("Warehouse Box")).not.toBeInTheDocument();
    expect(screen.queryByText("Shipping Envelope")).not.toBeInTheDocument();
  });

  it("treats a group literally named ungrouped as a group, not as the ungrouped filter", async () => {
    // "all" and "ungrouped" are legal group names. While the filter state was a bare string with
    // those two as sentinels, this template's own chip filtered the ungrouped set instead of the
    // group, and a group named "all" could not be filtered at all (#164 review).
    const collide = [
      {
        id: "named",
        name: "Named",
        group: "ungrouped",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
      {
        id: "loose",
        name: "Loose",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
    ];
    stubFetch({ templates: collide });
    renderPage();
    await screen.findByText("Named");
    const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
    fireEvent.click(within(groupToolbar).getByRole("button", { name: "ungrouped" }));
    await waitFor(() => expect(screen.queryByText("Loose")).not.toBeInTheDocument());
    expect(screen.getByText("Named")).toBeInTheDocument();

    fireEvent.click(within(groupToolbar).getByRole("button", { name: "Ungrouped" }));
    await waitFor(() => expect(screen.getByText("Loose")).toBeInTheDocument());
    expect(screen.queryByText("Named")).not.toBeInTheDocument();
  });

  it("omits Ungrouped filter button when all templates have a group", async () => {
    const allGrouped = [
      {
        id: "t1",
        name: "T1",
        group: "A",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
      {
        id: "t2",
        name: "T2",
        group: "B",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
    ];
    stubFetch({ templates: allGrouped });
    renderPage();
    await screen.findByText("T1");
    const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
    expect(within(groupToolbar).queryByRole("button", { name: "Ungrouped" })).not.toBeInTheDocument();
  });

  it("hides Favorites and Recents under group filter and restores them on All", async () => {
    const customTemplates = [
      {
        id: "t1",
        name: "T1",
        group: "Warehouse",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
      {
        id: "t2",
        name: "T2",
        group: "Shipping",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
    ];
    stubFetch({ templates: customTemplates, favorites: ["t1"], recent: ["t2"] });
    renderPage();

    expect(await screen.findByRole("region", { name: "Favorites" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Recent" })).toBeInTheDocument();

    const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
    fireEvent.click(within(groupToolbar).getByRole("button", { name: "Warehouse" }));

    // Favorites & Recent hidden under filter
    expect(screen.queryByRole("region", { name: "Favorites" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Recent" })).not.toBeInTheDocument();

    // Click All restores them
    fireEvent.click(within(groupToolbar).getByRole("button", { name: "All" }));
    expect(screen.getByRole("region", { name: "Favorites" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Recent" })).toBeInTheDocument();
  });

  it("moves a single template and updates its group card badge without reload", async () => {
    const calls = stubFetch();
    renderPage();
    await screen.findByText("Brother 24mm");

    const moveBtn = screen.getByRole("button", { name: "Move Brother 24mm" });
    fireEvent.click(moveBtn);

    const dialog = screen.getByRole("dialog", { name: "Move Brother 24mm" });
    const input = within(dialog).getByLabelText("Group name");
    fireEvent.change(input, { target: { value: "Packaging" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Move" }));

    await waitFor(() => {
      expect(
        calls.some(
          (c) =>
            c.method === "PUT" &&
            c.url === "/api/templates/brother_24mm_qr/group" &&
            JSON.stringify(c.body) === JSON.stringify({ group: "Packaging" }),
        ),
      ).toBe(true);
    });

    const packagingBadges = await screen.findAllByText("Packaging");
    expect(packagingBadges.length).toBeGreaterThan(0);
  });

  it("allows naming a new group or making ungrouped", async () => {
    const groupedTemplates = [
      {
        id: "t1",
        name: "T1",
        group: "ExistingGroup",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
    ];
    const calls = stubFetch({ templates: groupedTemplates });
    renderPage();
    await screen.findByText("T1");

    fireEvent.click(screen.getByRole("button", { name: "Move T1" }));
    const dialog = screen.getByRole("dialog", { name: "Move T1" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Make ungrouped" }));

    await waitFor(() => {
      expect(
        calls.some(
          (c) =>
            c.method === "PUT" &&
            c.url === "/api/templates/t1/group" &&
            JSON.stringify(c.body) === JSON.stringify({ group: null }),
        ),
      ).toBe(true);
    });
  });

  it("supports bulk selection and handles partial failure reporting", async () => {
    const bulkTemplates = [
      {
        id: "t1",
        name: "T1",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
      {
        id: "t2",
        name: "T2",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
    ];
    stubFetch({ templates: bulkTemplates, failMoveIds: ["t2"] });
    renderPage();

    await screen.findByText("T1");
    fireEvent.click(screen.getByRole("checkbox", { name: "Select T1" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Select T2" }));

    const selectionBar = screen.getByRole("region", { name: "Selection actions" });
    expect(within(selectionBar).getByText("2 selected")).toBeInTheDocument();

    fireEvent.click(within(selectionBar).getByRole("button", { name: "Move to…" }));

    const dialog = screen.getByRole("dialog", { name: "Move 2 templates" });
    const input = within(dialog).getByLabelText("Group name");
    fireEvent.change(input, { target: { value: "BulkGroup" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Move" }));

    expect(await screen.findByText(/Moved 1 templates\. Failed 1: t2/i)).toBeInTheDocument();
  });

  it("maintains favorite status when template is moved", async () => {
    stubFetch({ favorites: ["brother_24mm_qr"] });
    renderPage();

    const favRegion = await screen.findByRole("region", { name: "Favorites" });
    expect(within(favRegion).getByText("Brother 24mm")).toBeInTheDocument();

    // Move Brother to "Logistics"
    fireEvent.click(screen.getAllByRole("button", { name: "Move Brother 24mm" })[0]);
    const dialog = screen.getByRole("dialog", { name: "Move Brother 24mm" });
    const input = within(dialog).getByLabelText("Group name");
    fireEvent.change(input, { target: { value: "Logistics" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Move" }));

    // Still favorite after refetch
    await waitFor(() => {
      expect(screen.getByRole("region", { name: "Favorites" })).toBeInTheDocument();
    });
    expect(within(screen.getByRole("region", { name: "Favorites" })).getByText("Brother 24mm")).toBeInTheDocument();
  });
});
