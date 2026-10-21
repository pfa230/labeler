import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Activity } from "./Activity";
import type { ActivityEntry } from "../api/types";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const entry: ActivityEntry = {
  id: 102, time: "2026-10-20T09:15:04Z", actor_id: "u1", actor_name: "pat",
  action: "template.replace", target: "box", outcome: "failure",
  error_code: "TemplateInvalid", error_reason: "template_validation_failed",
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><Activity /></QueryClientProvider>);
}

describe("Activity", () => {
  afterEach(() => vi.unstubAllGlobals());

  // Fails when a recorded action's attribution, time or classification is missing from the page.
  it("identifies a failed action and its stable classification", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => json({ entries: [entry], next_before_id: null })));
    renderPage();
    const table = await screen.findByRole("table");
    for (const text of ["pat", "Replace template", "box", "Failure", "TemplateInvalid", "template_validation_failed"]) {
      expect(within(table).getByText(text)).toBeInTheDocument();
    }
    expect(table.querySelector("time")).toHaveAttribute("datetime", entry.time);
    expect(table.querySelector("time")?.textContent).not.toBe("");
    expect(screen.queryByRole("button", { name: "Older" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Newest" })).not.toBeInTheDocument();
  });

  // Fails if Older invents a cursor, Newest keeps it, or the final page offers another empty page.
  it("follows the supplied cursor and resets to newest", async () => {
    const fetchMock = vi.fn(async (url: string) => {
      if (url === "/api/activity") return json({ entries: [entry], next_before_id: 73 });
      if (url === "/api/activity?before_id=73") return json({
        entries: [{ ...entry, id: 72, target: "older-box", outcome: "success", error_code: undefined, error_reason: undefined }],
        next_before_id: null,
      });
      throw new Error(`Unexpected URL: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Older" }));
    expect(await screen.findByText("older-box")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledWith("/api/activity?before_id=73");
    expect(screen.getByText("Success")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Older" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Newest" }));
    expect(await screen.findByText("box")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Newest" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Older" })).toBeInTheDocument();
  });

  it("shows pending reads", () => {
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(() => {})));
    renderPage();
    expect(screen.getByText(/loading activity/i)).toBeInTheDocument();
  });

  it("shows empty history", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => json({ entries: [], next_before_id: null })));
    renderPage();
    expect(await screen.findByText("No activity recorded yet.")).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });

  it("distinguishes a failed read from empty history", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => json({ error: { code: "Internal", message: "Database unavailable" } }, 500)));
    renderPage();
    expect(await screen.findByRole("alert")).toHaveTextContent("Database unavailable");
    expect(screen.queryByText("No activity recorded yet.")).not.toBeInTheDocument();
  });
});
