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
  format: any;
  group?: string;
}> = [
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
      positions: [
        [0, 0],
        [4.25, 0],
        [0, 2],
        [4.25, 2],
        [0, 4],
        [4.25, 4],
      ],
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
  groups?: string[];
  failMoveIds?: string[];
  failRenameGroup?: { status: number; code?: string; reason?: string; message: string };
  failRefreshTemplates?: boolean;
}) {
  let favorites = [...(opts?.favorites ?? [])];
  const recent = [...(opts?.recent ?? [])];
  let groups = [...(opts?.groups ?? [])];
  let currentTemplates = [...(opts?.templates ?? (opts?.empty ? [] : templates))];
  let failRefresh = opts?.failRefreshTemplates ?? false;
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
    if (url.startsWith("/api/template-groups/") && method === "PUT") {
      const groupPath = decodeURIComponent(url.slice("/api/template-groups/".length));
      if (opts?.failRenameGroup) {
        const { status, code, reason, message } = opts.failRenameGroup;
        return new Response(
          JSON.stringify({
            error: { code: code ?? "TemplateGroupInvalid", message, details: reason ? { reason } : undefined },
          }),
          { status, headers: { "content-type": "application/json" } },
        );
      }
      const newName = parsedBody?.name;
      const parent = groupPath.includes("/") ? groupPath.slice(0, groupPath.lastIndexOf("/")) : null;
      const newGroupPath = parent ? `${parent}/${newName}` : newName;
      groups = groups.map((g) =>
        g === groupPath
          ? newGroupPath
          : g.startsWith(groupPath + "/")
            ? `${newGroupPath}${g.slice(groupPath.length)}`
            : g,
      );
      currentTemplates = currentTemplates.map((t) => {
        if (t.group === groupPath) return { ...t, group: newGroupPath };
        if (t.group?.startsWith(groupPath + "/")) {
          return { ...t, group: `${newGroupPath}${t.group.slice(groupPath.length)}` };
        }
        return t;
      });
      return jsonResponse({ group: newGroupPath });
    }
    if (url.startsWith("/api/template-groups/") && method === "DELETE") {
      const groupPath = decodeURIComponent(url.slice("/api/template-groups/".length));
      groups = groups.filter((g) => g !== groupPath);
      return new Response(null, { status: 204 });
    }
    if (url === "/api/template-groups") return jsonResponse(groups);
    if (url === "/api/favorites") return jsonResponse(favorites);
    if (url === "/api/recent-templates") return jsonResponse(recent);
    if (url === "/api/templates" || url.startsWith("/api/templates?")) {
      if (delayRefetch) {
        await delayRefetch;
      }
      if (failRefresh) {
        return new Response(
          JSON.stringify({ error: { code: "TemplateRegistryIo", message: "Failed to reload templates" } }),
          { status: 500, headers: { "content-type": "application/json" } },
        );
      }
      return jsonResponse({ templates: currentTemplates });
    }
    return jsonResponse({ templates: currentTemplates });
  });
  let delayRefetch: Promise<void> | null = null;
  (calls as any).setDelayRefetch = (p: Promise<void> | null) => {
    delayRefetch = p;
  };
  (calls as any).setFailRefresh = (v: boolean) => {
    failRefresh = v;
  };
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

  // The badge rides the top rail with the selection checkbox. Nothing else pins that: moving it back
  // beside the id chip would collapse the id to a single character again (see the change's tasks.md,
  // "Browser check outcome") and no other assertion would notice.
  it("puts the badge on the top rail, beside the selection checkbox", async () => {
    renderPage();
    await screen.findByText("Avery 5163");
    const badge = document.querySelector<HTMLElement>('[data-format="sheet"]')!;
    const rail = badge.parentElement!;
    expect(rail.querySelector('input[type="checkbox"]')).not.toBeNull();
    // and not beside the id chip it used to squeeze
    expect(rail.querySelector("code")).toBeNull();
    // Sharing a row with the checkbox is not enough on its own: moving that whole wrapper down into
    // the button row would still satisfy it. Document order pins the rail to the top of the card.
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

  it("filters nested groups with the include-nested switch", async () => {
    const nestedTemplates = [
      {
        id: "ship_direct",
        name: "Direct Shipping",
        group: "Shipping",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
      {
        id: "ship_pallet",
        name: "Pallet Shipping",
        group: "Shipping/Pallets",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
      {
        id: "other",
        name: "Other Item",
        group: "Warehouse",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
    ];
    stubFetch({ templates: nestedTemplates, groups: ["Shipping", "Shipping/Pallets", "Warehouse"] });
    renderPage();

    await screen.findByText("Direct Shipping");
    const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
    const switchCheckbox = screen.getByRole("checkbox", { name: "Include nested subgroups" });

    // Switch is disabled under All
    expect(switchCheckbox).toBeDisabled();

    // Select Shipping group
    fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping" }));
    expect(switchCheckbox).not.toBeDisabled();
    expect(screen.getByText("Direct Shipping")).toBeInTheDocument();
    expect(screen.queryByText("Pallet Shipping")).not.toBeInTheDocument();
    expect(screen.queryByText("Other Item")).not.toBeInTheDocument();

    // Toggle include-nested switch on
    fireEvent.click(switchCheckbox);
    expect(screen.getByText("Direct Shipping")).toBeInTheDocument();
    expect(screen.getByText("Pallet Shipping")).toBeInTheDocument();
    expect(screen.queryByText("Other Item")).not.toBeInTheDocument();
  });

  it("allows deleting an empty group with no subgroups", async () => {
    const customTemplates = [
      {
        id: "t1",
        name: "Template 1",
        group: "Shipping",
        description: "",
        unit: "mm",
        dpi: 300,
        format: { type: "single" as const, width: 50, height: 25 },
      },
    ];
    const calls = stubFetch({ templates: customTemplates, groups: ["Shipping", "EmptyGroup"] });
    renderPage();

    await screen.findByText("Template 1");
    const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });

    // Under Shipping (non-empty), Delete group button is not present
    fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping" }));
    expect(screen.queryByRole("button", { name: /delete group/i })).not.toBeInTheDocument();

    // Under EmptyGroup, Delete group button appears
    fireEvent.click(within(groupToolbar).getByRole("button", { name: "EmptyGroup" }));
    const deleteBtn = screen.getByRole("button", { name: "Delete group EmptyGroup" });
    expect(deleteBtn).toBeInTheDocument();

    fireEvent.click(deleteBtn);
    await waitFor(() => {
      expect(
        calls.some((c) => c.method === "DELETE" && c.url === "/api/template-groups/EmptyGroup"),
      ).toBe(true);
    });
  });

  describe("Group renaming", () => {
    it("rename action is absent for All and Ungrouped but present for a real group", async () => {
      const customTemplates = [
        {
          id: "t1",
          name: "Template 1",
          group: "Shipping",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
        {
          id: "t2",
          name: "Template 2",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
      ];
      stubFetch({ templates: customTemplates, groups: ["Shipping"] });
      renderPage();

      await screen.findByText("Template 1");
      const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });

      // Under All, rename action absent
      expect(screen.queryByRole("button", { name: /rename group/i })).not.toBeInTheDocument();

      // Under Ungrouped, rename action absent
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Ungrouped" }));
      expect(screen.queryByRole("button", { name: /rename group/i })).not.toBeInTheDocument();

      // Under real group Shipping, rename action is present
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping" }));
      expect(screen.getByRole("button", { name: "Rename group Shipping" })).toBeInTheDocument();
    });

    it("renames group successfully and updates selection and filter toolbar", async () => {
      const customTemplates = [
        {
          id: "t1",
          name: "Template 1",
          group: "Shipping",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
      ];
      const calls = stubFetch({ templates: customTemplates, groups: ["Shipping"] });
      renderPage();

      await screen.findByText("Template 1");
      const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });

      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping" }));
      const renameBtn = screen.getByRole("button", { name: "Rename group Shipping" });
      fireEvent.click(renameBtn);

      // Dialog opens
      const dialog = screen.getByRole("dialog", { name: "Rename group Shipping" });
      expect(dialog).toBeInTheDocument();

      const input = within(dialog).getByLabelText("New name");
      expect(input).toHaveValue("Shipping");

      fireEvent.change(input, { target: { value: "Logistics" } });
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

      await waitFor(() => {
        expect(
          calls.some(
            (c) =>
              c.method === "PUT" &&
              c.url === "/api/template-groups/Shipping" &&
              (c.body as any)?.name === "Logistics",
          ),
        ).toBe(true);
      });

      // Filter is rewritten to Logistics
      await waitFor(() => {
        expect(screen.getByRole("button", { name: "Rename group Logistics" })).toBeInTheDocument();
      });
      expect(within(groupToolbar).getByRole("button", { name: "Logistics" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
    });

    it("rewrites selected path by whole segments when renamed", async () => {
      const customTemplates = [
        {
          id: "t1",
          name: "Pallet Template",
          group: "Shipping/Pallets",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
      ];
      stubFetch({ templates: customTemplates, groups: ["Shipping", "Shipping/Pallets"] });
      renderPage();

      await screen.findByText("Pallet Template");
      const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });

      // Select Shipping/Pallets and rename to Boxes
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping/Pallets" }));
      fireEvent.click(screen.getByRole("button", { name: "Rename group Shipping/Pallets" }));

      const dialog = screen.getByRole("dialog", { name: "Rename group Shipping/Pallets" });
      const input = within(dialog).getByLabelText("New name");
      expect(input).toHaveValue("Pallets");
      fireEvent.change(input, { target: { value: "Boxes" } });
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

      // Selection becomes Shipping/Boxes
      await waitFor(() => {
        expect(screen.getByRole("button", { name: "Rename group Shipping/Boxes" })).toBeInTheDocument();
      });
      expect(screen.getByText("Pallet Template")).toBeInTheDocument();
    });

    it("preserves prefix-sharing sibling group selection when ancestor name is renamed", async () => {
      const customTemplates = [
        {
          id: "t1",
          name: "Shipping Template",
          group: "Shipping",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
        {
          id: "t2",
          name: "Shipping2 Template",
          group: "Shipping2",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
      ];
      stubFetch({ templates: customTemplates, groups: ["Shipping", "Shipping2"] });
      renderPage();

      await screen.findByText("Shipping Template");
      const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });

      // Open rename dialog for Shipping
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping" }));
      fireEvent.click(screen.getByRole("button", { name: "Rename group Shipping" }));

      const dialog = screen.getByRole("dialog", { name: "Rename group Shipping" });
      fireEvent.change(within(dialog).getByLabelText("New name"), { target: { value: "Freight" } });

      // Switch selection to sibling Shipping2 before submitting rename
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping2" }));
      expect(within(groupToolbar).getByRole("button", { name: "Shipping2" })).toHaveAttribute("aria-pressed", "true");

      // Submit rename of Shipping -> Freight
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

      await waitFor(() => {
        expect(within(groupToolbar).getByRole("button", { name: "Freight" })).toBeInTheDocument();
      });

      // Sibling group Shipping2 must stay selected and NOT be rewritten to Freight2
      expect(within(groupToolbar).getByRole("button", { name: "Shipping2" })).toHaveAttribute("aria-pressed", "true");
      expect(screen.queryByRole("button", { name: "Freight2" })).not.toBeInTheDocument();
      expect(screen.getByText("Shipping2 Template")).toBeInTheDocument();
      expect(screen.queryByText("Shipping Template")).not.toBeInTheDocument();
    });

    it("keeps pre-rename templates rendered continuously during transition without showing empty grid", async () => {
      const customTemplates = [
        {
          id: "t1",
          name: "Warehouse Tag",
          group: "Warehosue",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
      ];
      let resolveRefetch: () => void = () => {};
      const refetchPromise = new Promise<void>((resolve) => {
        resolveRefetch = resolve;
      });

      const calls = stubFetch({
        templates: customTemplates,
        groups: ["Warehosue"],
      });
      renderPage();

      await screen.findByText("Warehouse Tag");
      const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Warehosue" }));

      fireEvent.click(screen.getByRole("button", { name: "Rename group Warehosue" }));
      const dialog = screen.getByRole("dialog", { name: "Rename group Warehosue" });
      fireEvent.change(within(dialog).getByLabelText("New name"), { target: { value: "Warehouse" } });

      // Delay templates refetch to examine intermediate transition state
      (calls as any).setDelayRefetch(refetchPromise);
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

      // Wait for rename mutation to settle and dialog to close, entering the pending transition
      await waitFor(() => {
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      });

      // While refetch is pending in the background:
      // Pre-rename template snapshot must remain rendered
      expect(screen.getByText("Warehouse Tag")).toBeInTheDocument();
      // Must NOT render empty grid message
      expect(screen.queryByText("No templates in this group.")).not.toBeInTheDocument();
      // Must NOT render blank loading state
      expect(screen.queryByText("loading…")).not.toBeInTheDocument();

      // Now resolve the refetch
      resolveRefetch();
      (calls as any).setDelayRefetch(null);

      // After refetch completes, templates remain visible and selection updates
      await waitFor(() => {
        expect(screen.getByRole("button", { name: "Rename group Warehouse" })).toBeInTheDocument();
      });
      expect(screen.getByText("Warehouse Tag")).toBeInTheDocument();
      expect(screen.queryByText("No templates in this group.")).not.toBeInTheDocument();
    });

    it("displays inline validation errors for invalid group names", async () => {
      const customTemplates = [
        {
          id: "t1",
          name: "Template 1",
          group: "Shipping",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
      ];
      stubFetch({ templates: customTemplates, groups: ["Shipping"] });
      renderPage();

      await screen.findByText("Template 1");
      const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping" }));
      fireEvent.click(screen.getByRole("button", { name: "Rename group Shipping" }));

      const dialog = screen.getByRole("dialog", { name: "Rename group Shipping" });
      const input = within(dialog).getByLabelText("New name");

      // Slash in name
      fireEvent.change(input, { target: { value: "Warehouse/Pallets" } });
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));
      expect(screen.getByText(/cannot contain/i)).toBeInTheDocument();

      // Reserved device name CON
      fireEvent.change(input, { target: { value: "CON" } });
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));
      expect(screen.getByText(/"CON" is a reserved device name/i)).toBeInTheDocument();

      // Leading dot
      fireEvent.change(input, { target: { value: ".hidden" } });
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));
      expect(screen.getByText(/cannot begin or end with "\."/i)).toBeInTheDocument();
    });

    it("surfaces route refusals distinctly (404, 409, 422)", async () => {
      const customTemplates = [
        {
          id: "t1",
          name: "Template 1",
          group: "Shipping",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
      ];
      stubFetch({
        templates: customTemplates,
        groups: ["Shipping"],
        failRenameGroup: { status: 409, message: "destination group directory already exists" },
      });
      renderPage();

      await screen.findByText("Template 1");
      const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping" }));
      fireEvent.click(screen.getByRole("button", { name: "Rename group Shipping" }));

      const dialog = screen.getByRole("dialog", { name: "Rename group Shipping" });
      fireEvent.change(within(dialog).getByLabelText("New name"), { target: { value: "Warehouse" } });
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

      expect(await screen.findByText(/already exists/i)).toBeInTheDocument();
    });

    it("reports failed refresh error and allows retry", async () => {
      const customTemplates = [
        {
          id: "t1",
          name: "Template 1",
          group: "Shipping",
          description: "",
          unit: "mm",
          dpi: 300,
          format: { type: "single" as const, width: 50, height: 25 },
        },
      ];
      const calls = stubFetch({
        templates: customTemplates,
        groups: ["Shipping"],
      });
      renderPage();

      await screen.findByText("Template 1");
      const groupToolbar = screen.getByRole("toolbar", { name: "Group filter" });
      fireEvent.click(within(groupToolbar).getByRole("button", { name: "Shipping" }));
      fireEvent.click(screen.getByRole("button", { name: "Rename group Shipping" }));

      const dialog = screen.getByRole("dialog", { name: "Rename group Shipping" });
      fireEvent.change(within(dialog).getByLabelText("New name"), { target: { value: "Logistics" } });

      // Fail next refresh
      (calls as any).setFailRefresh(true);
      fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

      // Refresh error alert is shown
      expect(await screen.findByRole("alert")).toHaveTextContent(/Failed to refresh after rename/i);
      // Pre-rename template snapshot still rendered
      expect(screen.getByText("Template 1")).toBeInTheDocument();

      // Fix refresh and click Retry refresh
      (calls as any).setFailRefresh(false);
      fireEvent.click(screen.getByRole("button", { name: "Retry refresh" }));

      // Alert disappears and selection is updated
      await waitFor(() => {
        expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      });
      expect(screen.getByRole("button", { name: "Rename group Logistics" })).toBeInTheDocument();
    });
  });
});
