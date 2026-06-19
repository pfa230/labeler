import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { UsersSection } from "./UsersSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type U = { id: string; username: string };

// Stateful stub: POST/DELETE mutate `state`, GET returns it, so an invalidate+refetch shows the real
// post-mutation table. The username unique check returns a 409 the form must surface inline.
function stubFetch() {
  let state: U[] = [
    { id: "u1", username: "alice" },
    { id: "u2", username: "bob" },
  ];
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/auth/me")) return json({ authed: true, needsSetup: false, me: { id: "u1", username: "alice" } });
    if (url.startsWith("/api/users/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/users/".length));
      if (state.length <= 1) return json({ error: { code: "Conflict", message: "cannot delete the last user" } }, 409);
      state = state.filter((u) => u.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/users") && method === "POST") {
      const body = JSON.parse(init!.body as string) as { username: string };
      if (state.some((u) => u.username === body.username)) {
        return json({ error: { code: "Conflict", message: "username already exists" } }, 409);
      }
      const created = { id: `id-${body.username}`, username: body.username };
      state = [...state, created];
      return json(created, 201);
    }
    if (url.startsWith("/api/users")) return json(state);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <UsersSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string, method: string) =>
  [...fetchMock.mock.calls].reverse().find(([u, i]) => String(u).startsWith(path) && ((i as RequestInit)?.method ?? "GET").toUpperCase() === method);

describe("UsersSection", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("lists users", async () => {
    renderSection();
    expect(await screen.findByText("alice")).toBeInTheDocument();
    expect(screen.getByText("bob")).toBeInTheDocument();
  });

  it("adds a user via POST", async () => {
    renderSection();
    await screen.findByText("alice");
    fireEvent.change(screen.getByLabelText(/new username/i), { target: { value: "carol" } });
    fireEvent.change(screen.getByLabelText(/new user password/i), { target: { value: "pw" } });
    fireEvent.click(screen.getByRole("button", { name: /add user/i }));
    await waitFor(() => expect(lastCall("/api/users", "POST")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/users", "POST")![1] as RequestInit).body as string);
    expect(body).toEqual({ username: "carol", password: "pw" });
    expect(await screen.findByText("carol")).toBeInTheDocument();
  });

  it("surfaces a duplicate-username 409 inline", async () => {
    renderSection();
    await screen.findByText("alice");
    fireEvent.change(screen.getByLabelText(/new username/i), { target: { value: "bob" } });
    fireEvent.change(screen.getByLabelText(/new user password/i), { target: { value: "pw" } });
    fireEvent.click(screen.getByRole("button", { name: /add user/i }));
    expect(await screen.findByText(/username already exists/i, { selector: "p" })).toBeInTheDocument();
  });

  it("deletes a user via DELETE after a confirm", async () => {
    renderSection();
    const row = (await screen.findByText("bob")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    await waitFor(() => expect(lastCall("/api/users/u2", "DELETE")).toBeTruthy());
    await waitFor(() => expect(screen.queryByText("bob")).not.toBeInTheDocument());
  });

  it("disables delete for the current user", async () => {
    renderSection();
    const aliceRow = (await screen.findByText("alice")).closest("tr") as HTMLElement;
    const bobRow = (await screen.findByText("bob")).closest("tr") as HTMLElement;
    expect(within(aliceRow).getByText(/\(you\)/i)).toBeInTheDocument();
    expect(within(aliceRow).getByRole("button", { name: /^delete$/i })).toBeDisabled();
    expect(within(bobRow).getByRole("button", { name: /^delete$/i })).toBeEnabled();
  });

  it("shows the last-user 409 message from a rejected delete", async () => {
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      // me is a different principal than "solo" so the self-delete guard does not disable the button;
      // this test exercises the UI's surfacing of the backend last-user 409.
      if (url.startsWith("/api/auth/me")) return json({ authed: true, needsSetup: false, me: { id: "admin", username: "admin" } });
      if (url.startsWith("/api/users/") && method === "DELETE") {
        return json({ error: { code: "Conflict", message: "cannot delete the last user" } }, 409);
      }
      if (url.startsWith("/api/users")) return json([{ id: "only", username: "solo" }]);
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const row = (await screen.findByText("solo")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    expect(await screen.findByText(/cannot delete the last user/i)).toBeInTheDocument();
  });

  it("changes the password via POST /auth/password", async () => {
    const pwMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (url.startsWith("/api/auth/me")) return json({ authed: true, needsSetup: false, me: { id: "u1", username: "alice" } });
      if (url.startsWith("/api/auth/password") && method === "POST") return json({ ok: true });
      if (url.startsWith("/api/users")) return json([{ id: "u1", username: "alice" }]);
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", pwMock);
    renderSection();
    await screen.findByText("alice");
    fireEvent.change(screen.getByLabelText(/current password/i), { target: { value: "old" } });
    fireEvent.change(screen.getByLabelText(/new password value/i), { target: { value: "new" } });
    fireEvent.click(screen.getByRole("button", { name: /change password/i }));
    await waitFor(() => {
      const call = [...pwMock.mock.calls].reverse().find(([u]) => String(u).startsWith("/api/auth/password"));
      expect(call).toBeTruthy();
      expect(JSON.parse((call![1] as RequestInit).body as string)).toEqual({ current_password: "old", new_password: "new" });
    });
  });
});
