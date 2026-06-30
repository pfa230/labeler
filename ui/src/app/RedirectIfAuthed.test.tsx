import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RedirectIfAuthed } from "./RedirectIfAuthed";

type AuthState = { authed: boolean; needsSetup: boolean };

function mockAuth(state: AuthState) {
  global.fetch = vi.fn(async (input: RequestInfo | URL) => {
    if (String(input).includes("/api/auth/me")) {
      return new Response(JSON.stringify(state), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    return new Response("{}", { status: 200 });
  }) as unknown as typeof fetch;
}

function renderGuarded() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/login"]}>
        <Routes>
          <Route element={<RedirectIfAuthed />}>
            <Route path="/login" element={<div>login page</div>} />
          </Route>
          <Route path="/" element={<div>home page</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.restoreAllMocks());

describe("RedirectIfAuthed", () => {
  it("redirects an authed user away from a public route to /", async () => {
    mockAuth({ authed: true, needsSetup: false });
    renderGuarded();
    expect(await screen.findByText("home page")).toBeTruthy();
  });

  it("renders the public route when not authed", async () => {
    mockAuth({ authed: false, needsSetup: true });
    renderGuarded();
    expect(await screen.findByText("login page")).toBeTruthy();
  });
});
