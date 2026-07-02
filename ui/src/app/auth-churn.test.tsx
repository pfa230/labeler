import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RedirectIfAuthed } from "./RedirectIfAuthed";
import type { AuthState } from "../api/auth";

describe("auth churn on 401", () => {
  it("guarded /login renders when the auth cache is cleared to authed:false", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    // Simulate the post-401 cache state the main.tsx labeler:unauthenticated listener writes.
    qc.setQueryData<AuthState>(["auth"], { authed: false, needsSetup: false });
    render(
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
    // With authed:false cached, the guard renders the Outlet (Login), not a bounce to "/".
    expect(await screen.findByText("login page")).toBeTruthy();
  });
});
