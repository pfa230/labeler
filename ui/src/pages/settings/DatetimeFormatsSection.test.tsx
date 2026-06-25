import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { DatetimeFormatsSection } from "./DatetimeFormatsSection";

// Mock the queries module so we can control previewDatetimeFormat without fetch.
vi.mock("../../api/queries", async (importOriginal) => {
  const mod = await importOriginal<typeof import("../../api/queries")>();
  return {
    ...mod,
    previewDatetimeFormat: vi.fn(),
  };
});

import * as queries from "../../api/queries";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function stubFetch(initial: Record<string, string>, is_default = true) {
  const state = { value: { ...initial }, is_default };
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/settings/datetime_formats") && method === "PUT") {
      state.value = JSON.parse(init!.body as string).value as Record<string, string>;
      state.is_default = false;
      return json({ value: state.value, is_default: false });
    }
    if (url.startsWith("/api/settings/datetime_formats") && method === "DELETE") {
      state.value = { short_date: "%Y-%m-%d" };
      state.is_default = true;
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/settings")) {
      return json({ datetime_formats: { value: { ...state.value }, is_default: state.is_default } });
    }
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <DatetimeFormatsSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("DatetimeFormatsSection", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it("shows mocked preview sample for a valid pattern", async () => {
    vi.mocked(queries.previewDatetimeFormat).mockResolvedValue({ sample: "2026-06-25" });
    vi.stubGlobal("fetch", stubFetch({ short_date: "%Y-%m-%d" }));
    renderSection();

    // Wait for the row to appear
    const patternInput = await screen.findByLabelText("strftime pattern");
    expect(patternInput).toHaveValue("%Y-%m-%d");

    // Debounce fires after 400ms; use fake timers to advance
    await act(async () => {
      await vi.waitFor(() => expect(screen.getByText("2026-06-25")).toBeInTheDocument(), { timeout: 2000 });
    });
  });

  it("shows error message for an invalid pattern", async () => {
    vi.mocked(queries.previewDatetimeFormat).mockRejectedValue(new Error("Invalid strftime pattern: %Q"));
    vi.stubGlobal("fetch", stubFetch({ bad: "%Q" }));
    renderSection();

    await screen.findByLabelText("strftime pattern");

    await act(async () => {
      await vi.waitFor(() => expect(screen.getByText("Invalid strftime pattern: %Q")).toBeInTheDocument(), { timeout: 2000 });
    });
  });

  it("saves rows via PUT when Save is clicked", async () => {
    vi.mocked(queries.previewDatetimeFormat).mockResolvedValue({ sample: "2026-06-25" });
    const fetchMock = stubFetch({ short_date: "%Y-%m-%d" });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();

    await screen.findByLabelText("strftime pattern");
    fireEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      const putCall = [...fetchMock.mock.calls].reverse().find(([u, i]) =>
        String(u).startsWith("/api/settings/datetime_formats") && ((i as RequestInit)?.method ?? "GET").toUpperCase() === "PUT"
      );
      expect(putCall).toBeTruthy();
    });
  });
});
