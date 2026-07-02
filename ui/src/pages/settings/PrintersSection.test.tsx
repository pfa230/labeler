import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { PrintersSection } from "./PrintersSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type P = { id: string; name: string; kind: string; config: { uri: string }; enabled: boolean };

// Stateful stub: POST/PUT/DELETE mutate `state`, GET returns it, so an invalidate+refetch shows the
// real post-mutation table. `/printers/probe` returns a canned reachable printer.
function stubFetch() {
  let state: P[] = [{ id: "front", name: "Front Desk", kind: "cups", config: { uri: "ipp://x/y" }, enabled: true }];
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url === "/api/printers/probe" && method === "POST") {
      return json({ status: "ok", capabilities: { model: "Brother PT-2730", media_width_mm: 24, resolution_dpi: 180, color: "bilevel", accepts_png: true } });
    }
    if (url.startsWith("/api/printers/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/printers/".length));
      state = state.filter((p) => p.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/printers/") && method === "PUT") {
      const p = JSON.parse(init!.body as string) as P;
      state = state.map((x) => (x.id === p.id ? p : x));
      return json(p);
    }
    if (url.startsWith("/api/printers") && method === "POST") {
      const p = JSON.parse(init!.body as string) as P;
      state = [...state, p];
      return json(p, 201);
    }
    if (url.startsWith("/api/printers")) return json(state);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <PrintersSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string, method: string) =>
  [...fetchMock.mock.calls].reverse().find(([u, i]) => String(u).startsWith(path) && ((i as RequestInit)?.method ?? "GET").toUpperCase() === method);

describe("PrintersSection", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("lists printers with name, kind and uri", async () => {
    renderSection();
    expect(await screen.findByText("Front Desk")).toBeInTheDocument();
    expect(screen.getByText("ipp://x/y")).toBeInTheDocument();
  });

  it("adds a printer via POST with a cups config", async () => {
    renderSection();
    await screen.findByText("Front Desk");
    fireEvent.click(screen.getByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "back" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Back Office" } });
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://b/q" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers", "POST")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers", "POST")![1] as RequestInit).body as string);
    expect(body).toEqual({ id: "back", name: "Back Office", kind: "cups", config: { uri: "ipp://b/q" }, enabled: true });
    expect(await screen.findByText("Back Office")).toBeInTheDocument();
  });

  it("edits a printer via PUT; the id is structurally immutable (no field on edit)", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    expect(screen.queryByLabelText(/printer id/i)).toBeNull(); // no id field to change on edit
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Lobby" } });
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://x/z" } });
    fireEvent.click(screen.getByLabelText("enabled")); // toggle true -> false
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers/front", "PUT")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers/front", "PUT")![1] as RequestInit).body as string);
    expect(body).toEqual({ id: "front", name: "Lobby", kind: "cups", config: { uri: "ipp://x/z" }, enabled: false });
    expect(await screen.findByText("Lobby")).toBeInTheDocument();
  });

  it("preserves existing auth config on edit even though the card hides it", async () => {
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (url.startsWith("/api/printers/") && method === "PUT") return json(JSON.parse(init!.body as string));
      if (url.startsWith("/api/printers")) {
        return json([{ id: "auth", name: "Auth", kind: "cups", config: { uri: "ipps://h/q", username: "u", ca_cert: "PEM", insecure: true }, enabled: true }]);
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const row = (await screen.findByText("Auth")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Auth2" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers/auth", "PUT")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers/auth", "PUT")![1] as RequestInit).body as string);
    // username/ca_cert/insecure carried forward untouched; password never echoed.
    expect(body.config).toEqual({ uri: "ipps://h/q", username: "u", ca_cert: "PEM", insecure: true });
  });

  it("blocks an invalid printer id client-side", async () => {
    renderSection();
    await screen.findByText("Front Desk");
    fireEvent.click(screen.getByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "bad id" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "X" } });
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://b/q" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText(/id must contain only/i)).toBeInTheDocument();
    expect(lastCall("/api/printers", "POST")).toBeUndefined();
  });

  it("cancels then deletes a printer after an inline confirm", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /cancel/i }));
    expect(lastCall("/api/printers/front", "DELETE")).toBeUndefined();
    expect(screen.getByText("Front Desk")).toBeInTheDocument();
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    await waitFor(() => expect(lastCall("/api/printers/front", "DELETE")).toBeTruthy());
    await waitFor(() => expect(screen.queryByText("Front Desk")).not.toBeInTheDocument());
  });

  it("closes the edit form when the printer being edited is deleted", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    expect(screen.getByLabelText(/address/i)).toBeInTheDocument(); // form is open
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    await waitFor(() => expect(screen.queryByLabelText(/address/i)).not.toBeInTheDocument());
  });

  it("shows the capabilities strip after a successful test", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://ptouch:8000/ipp/print" } });
    fireEvent.click(screen.getByRole("button", { name: /test connection/i }));
    expect(await screen.findByText(/Brother PT-2730/)).toBeInTheDocument();
    expect(screen.getByText(/180 dpi/)).toBeInTheDocument();
  });

  it("shows an inline error when probe is unreachable", async () => {
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (url === "/api/printers/probe" && method === "POST") {
        return json({ status: "unreachable", detail: "connection refused" });
      }
      if (url.startsWith("/api/printers")) return json([]);
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://nope:8000/ipp/print" } });
    fireEvent.click(screen.getByRole("button", { name: /test connection/i }));
    expect(await screen.findByText(/connection refused/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^save$/i })).toBeEnabled(); // save still allowed
  });

  it("keeps the override disclosure collapsed by default", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add printer/i }));
    expect(screen.queryByLabelText(/color mode/i)).toBeNull(); // hidden until opened
    fireEvent.click(screen.getByRole("button", { name: /advanced/i }));
    expect(screen.getByLabelText(/color mode/i)).toBeInTheDocument();
  });

  it("submits a bilevel render profile from the advanced overrides", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "bl" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "BL" } });
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://h/q" } });
    fireEvent.click(screen.getByRole("button", { name: /advanced/i }));
    fireEvent.change(screen.getByLabelText(/color mode/i), { target: { value: "bilevel" } });
    fireEvent.change(screen.getByLabelText(/print resolution/i), { target: { value: "203" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers", "POST")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers", "POST")![1] as RequestInit).body as string);
    expect(body.config.render).toEqual({ color_mode: "bilevel", resolution: 203 });
  });

  it("omits render when color mode is auto (the default)", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "c" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "C" } });
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://h/q" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers", "POST")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers", "POST")![1] as RequestInit).body as string);
    expect("render" in body.config).toBe(false);
  });

  it("sets a printer as the default via its radio", async () => {
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (/\/api\/printers\/.+\/default$/.test(url) && method === "POST") {
        return new Response(null, { status: 204 });
      }
      if (url.startsWith("/api/printers")) {
        return json([{ id: "front", name: "Front Desk", kind: "cups", config: { uri: "ipp://x/y" }, enabled: true, is_default: false }]);
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByLabelText("default Front Desk"));
    await waitFor(() => expect(lastCall("/api/printers/front/default", "POST")).toBeTruthy());
  });

  it("shows a server validation error inline when save is rejected", async () => {
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (url.startsWith("/api/printers") && method === "POST") {
        return json({ error: { code: "PrinterInvalid", message: "cups uri rejected by server" } }, 422);
      }
      if (url.startsWith("/api/printers")) return json([]);
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "back" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Back" } });
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://ok" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText(/cups uri rejected by server/i, { selector: "p" })).toBeInTheDocument();
  });
});
