import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { PrintersSection } from "./PrintersSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type P = { id: string; name: string; kind: string; config: { uri: string }; enabled: boolean };

// Stateful stub: POST/PUT/DELETE mutate `state`, GET returns it, so an invalidate+refetch shows the
// real post-mutation table (this is what proves add/edit/delete actually took effect, and that the
// 204 DELETE path in `del` does not try to parse a body).
function stubFetch() {
  let state: P[] = [{ id: "front", name: "Front Desk", kind: "cups", config: { uri: "ipp://x/y" }, enabled: true }];
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
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
    fireEvent.change(screen.getByLabelText(/cups uri/i), { target: { value: "ipp://b/q" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers", "POST")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers", "POST")![1] as RequestInit).body as string);
    expect(body).toEqual({ id: "back", name: "Back Office", kind: "cups", config: { uri: "ipp://b/q" }, enabled: true });
    // the new printer appears in the refetched table
    expect(await screen.findByText("Back Office")).toBeInTheDocument();
  });

  it("edits a printer via PUT with an immutable id", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    expect(screen.getByLabelText(/printer id/i)).toBeDisabled(); // id is immutable on edit
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Lobby" } });
    fireEvent.change(screen.getByLabelText(/cups uri/i), { target: { value: "ipp://x/z" } });
    fireEvent.click(screen.getByLabelText("enabled")); // toggle true -> false
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers/front", "PUT")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers/front", "PUT")![1] as RequestInit).body as string);
    expect(body).toEqual({ id: "front", name: "Lobby", kind: "cups", config: { uri: "ipp://x/z" }, enabled: false });
    expect(await screen.findByText("Lobby")).toBeInTheDocument();
  });

  it("blocks an invalid printer id client-side", async () => {
    renderSection();
    await screen.findByText("Front Desk");
    fireEvent.click(screen.getByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "bad id" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "X" } });
    fireEvent.change(screen.getByLabelText(/cups uri/i), { target: { value: "ipp://b/q" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText(/id must contain only/i)).toBeInTheDocument();
    expect(lastCall("/api/printers", "POST")).toBeUndefined();
  });

  it("cancels then deletes a printer after an inline confirm", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    // Delete -> Cancel: no DELETE issued, row stays.
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /cancel/i }));
    expect(lastCall("/api/printers/front", "DELETE")).toBeUndefined();
    expect(screen.getByText("Front Desk")).toBeInTheDocument();
    // Delete -> Confirm: DELETE issued (204, no body), row removed after the refetch.
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    await waitFor(() => expect(lastCall("/api/printers/front", "DELETE")).toBeTruthy());
    await waitFor(() => expect(screen.queryByText("Front Desk")).not.toBeInTheDocument());
  });

  it("closes the edit form when the printer being edited is deleted", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    expect(screen.getByLabelText(/printer id/i)).toBeInTheDocument(); // form is open
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    // deleting the edited printer closes the now-stale form (otherwise Save would PUT a 404).
    await waitFor(() => expect(screen.queryByLabelText(/printer id/i)).not.toBeInTheDocument());
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
    fireEvent.change(screen.getByLabelText(/cups uri/i), { target: { value: "ipp://ok" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    // the 422 message surfaces inline in the form's <p> (onError -> setError); the toast shows it too.
    expect(await screen.findByText(/cups uri rejected by server/i, { selector: "p" })).toBeInTheDocument();
  });
});
