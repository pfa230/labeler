import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { PrintersSection } from "./PrintersSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type P = {
  id: string;
  name: string;
  uri: string;
  username?: string;
  ca_cert?: string;
  insecure: boolean;
  render?: { color_mode?: string; resolution?: number };
};
type Setting = { value: string | null; is_default: boolean };

const FRONT: P = { id: "front", name: "Front Desk", uri: "ipp://x/y", insecure: false };

// Stateful stub mirroring the server: POST/PUT/DELETE mutate `printers`, GET returns them, so an
// invalidate+refetch shows the real post-mutation table. `default_printer_id` lives in the settings
// map and a printer DELETE clears it when it named that printer. `/printers/probe` returns a canned
// reachable printer.
function stubFetch({ printers = [FRONT], defaultPrinterId = null }: { printers?: P[]; defaultPrinterId?: string | null } = {}) {
  let state = [...printers];
  let defaultPrinter: Setting = { value: defaultPrinterId, is_default: defaultPrinterId === null };
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url === "/api/settings" && method === "GET") return json({ default_printer_id: defaultPrinter });
    if (url === "/api/settings/default_printer_id" && method === "PUT") {
      defaultPrinter = { value: (JSON.parse(init!.body as string) as { value: string }).value, is_default: false };
      return json(defaultPrinter);
    }
    if (url === "/api/settings/default_printer_id" && method === "DELETE") {
      defaultPrinter = { value: null, is_default: true };
      return new Response(null, { status: 204 });
    }
    if (url === "/api/printers/probe" && method === "POST") {
      return json({ status: "ok", capabilities: { model: "Brother PT-2730", media_width_mm: 24, resolution_dpi: 180, color: "bilevel", accepts_png: true } });
    }
    if (url.startsWith("/api/printers/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/printers/".length));
      state = state.filter((p) => p.id !== id);
      if (defaultPrinter.value === id) defaultPrinter = { value: null, is_default: true };
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/printers/") && method === "PUT") {
      const id = decodeURIComponent(url.slice("/api/printers/".length));
      const p = { insecure: false, ...JSON.parse(init!.body as string), id } as P;
      state = state.map((x) => (x.id === id ? p : x));
      return json(p);
    }
    if (url === "/api/printers" && method === "POST") {
      const p = { insecure: false, ...JSON.parse(init!.body as string) } as P;
      state = [...state, p];
      return json(p, 201);
    }
    if (url === "/api/printers") return json(state);
    throw new Error(`unexpected fetch: ${method} ${url}`);
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

  it("lists printers with name and uri", async () => {
    renderSection();
    expect(await screen.findByText("Front Desk")).toBeInTheDocument();
    expect(screen.getByText("ipp://x/y")).toBeInTheDocument();
  });

  it("adds a printer via POST with a flat body", async () => {
    renderSection();
    await screen.findByText("Front Desk");
    fireEvent.click(screen.getByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "back" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Back Office" } });
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://b/q" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers", "POST")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers", "POST")![1] as RequestInit).body as string);
    expect(body).toEqual({ id: "back", name: "Back Office", uri: "ipp://b/q" });
    expect(await screen.findByText("Back Office")).toBeInTheDocument();
  });

  it("edits a printer via PUT without an id; the id has no field on edit", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    expect(screen.queryByLabelText(/printer id/i)).toBeNull(); // no id field to change on edit
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Lobby" } });
    fireEvent.change(screen.getByLabelText(/address/i), { target: { value: "ipp://x/z" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers/front", "PUT")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers/front", "PUT")![1] as RequestInit).body as string);
    expect(body).toEqual({ name: "Lobby", uri: "ipp://x/z", insecure: false });
    expect(await screen.findByText("Lobby")).toBeInTheDocument();
  });

  it("puts back the stored username, ca_cert, insecure and render on edit, with no id or password", async () => {
    const stored: P = {
      id: "auth",
      name: "Auth",
      uri: "ipps://h/q",
      username: "u",
      ca_cert: "PEM",
      insecure: true,
      render: { color_mode: "bilevel", resolution: 203 },
    };
    fetchMock = stubFetch({ printers: [stored] });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const row = (await screen.findByText("Auth")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Auth2" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers/auth", "PUT")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers/auth", "PUT")![1] as RequestInit).body as string);
    expect(body).toEqual({
      name: "Auth2",
      uri: "ipps://h/q",
      username: "u",
      ca_cert: "PEM",
      insecure: true,
      render: { color_mode: "bilevel", resolution: 203 },
    });
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
      if (url === "/api/settings") return json({ default_printer_id: { value: null, is_default: true } });
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
    expect(body.render).toEqual({ color_mode: "bilevel", resolution: 203 });
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
    expect("render" in body).toBe(false);
  });

  it("writes default_printer_id from a printer's radio and clears it from No default printer", async () => {
    renderSection();
    fireEvent.click(await screen.findByLabelText("default Front Desk"));
    await waitFor(() => expect(lastCall("/api/settings/default_printer_id", "PUT")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/settings/default_printer_id", "PUT")![1] as RequestInit).body as string);
    expect(body).toEqual({ value: "front" });
    await waitFor(() => expect(screen.getByLabelText("default Front Desk")).toBeChecked());
    fireEvent.click(screen.getByLabelText("no default printer"));
    await waitFor(() => expect(lastCall("/api/settings/default_printer_id", "DELETE")).toBeTruthy());
    await waitFor(() => expect(screen.getByLabelText("no default printer")).toBeChecked());
  });

  it("checks No default printer after the default printer is deleted", async () => {
    fetchMock = stubFetch({
      printers: [FRONT, { id: "back", name: "Back Office", uri: "ipp://b/q", insecure: false }],
      defaultPrinterId: "front",
    });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    await waitFor(() => expect(screen.getByLabelText("default Front Desk")).toBeChecked());
    const row = screen.getByText("Front Desk").closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    await waitFor(() => expect(screen.queryByText("Front Desk")).not.toBeInTheDocument());
    await waitFor(() => expect(screen.getByLabelText("no default printer")).toBeChecked());
  });

  it("shows a server validation error inline when save is rejected", async () => {
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (url.startsWith("/api/printers") && method === "POST") {
        return json({ error: { code: "InvalidRequest", message: "cups uri rejected by server", details: { reason: "printer_invalid" } } }, 400);
      }
      if (url.startsWith("/api/printers")) return json([]);
      if (url === "/api/settings") return json({ default_printer_id: { value: null, is_default: true } });
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
