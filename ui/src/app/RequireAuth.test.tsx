import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RequireAuth } from "./RequireAuth";

function stubAuth(state: { authed: boolean; needsSetup: boolean; me?: { id: string; username: string } }) {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/auth/me")) {
      return new Response(JSON.stringify(state), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderGuard() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/login" element={<h1>Sign in</h1>} />
          <Route element={<RequireAuth />}>
            <Route index element={<div>protected child</div>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("RequireAuth", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("redirects to /login when not authed", async () => {
    vi.stubGlobal("fetch", stubAuth({ authed: false, needsSetup: false }));
    renderGuard();
    expect(await screen.findByRole("heading", { name: /sign in/i })).toBeInTheDocument();
  });

  it("renders the child when authed", async () => {
    vi.stubGlobal("fetch", stubAuth({ authed: true, needsSetup: false, me: { id: "u1", username: "alice" } }));
    renderGuard();
    expect(await screen.findByText(/protected child/i)).toBeInTheDocument();
  });
});
