import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
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

// Advance fake timers (and flush the microtasks the query/preview promises chain on) until the
// predicate holds or we exceed `steps` 50ms ticks. Keeps the debounced preview deterministic.
async function advanceUntil(predicate: () => boolean, steps = 60) {
  for (let i = 0; i < steps; i++) {
    if (predicate()) return;
    await vi.advanceTimersByTimeAsync(50);
  }
  if (!predicate()) throw new Error("advanceUntil: predicate never became true");
}

describe("DatetimeFormatsSection", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("shows mocked preview sample for a valid pattern", async () => {
    vi.mocked(queries.previewDatetimeFormat).mockResolvedValue({ sample: "2026-06-25" });
    vi.stubGlobal("fetch", stubFetch({ short_date: "%Y-%m-%d" }));
    renderSection();

    // Query resolves and the row mounts with its pattern.
    await advanceUntil(() => screen.queryByLabelText("strftime pattern") !== null);
    expect(screen.getByLabelText("strftime pattern")).toHaveValue("%Y-%m-%d");

    // Cross the 400ms debounce so the (mocked) preview call fires and its sample renders.
    await advanceUntil(() => screen.queryByText("2026-06-25") !== null);
    expect(screen.getByText("2026-06-25")).toBeInTheDocument();
    expect(queries.previewDatetimeFormat).toHaveBeenCalledWith("%Y-%m-%d");
  });

  it("shows error message for an invalid pattern", async () => {
    vi.mocked(queries.previewDatetimeFormat).mockRejectedValue(new Error("Invalid strftime pattern: %Q"));
    vi.stubGlobal("fetch", stubFetch({ bad: "%Q" }));
    renderSection();

    await advanceUntil(() => screen.queryByLabelText("strftime pattern") !== null);
    await advanceUntil(() => screen.queryByText("Invalid strftime pattern: %Q") !== null);
    expect(screen.getByText("Invalid strftime pattern: %Q")).toBeInTheDocument();
    expect(queries.previewDatetimeFormat).toHaveBeenCalledWith("%Q");
  });

  it("saves rows via PUT when Save is clicked", async () => {
    vi.mocked(queries.previewDatetimeFormat).mockResolvedValue({ sample: "2026-06-25" });
    const fetchMock = stubFetch({ short_date: "%Y-%m-%d" });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();

    await advanceUntil(() => screen.queryByLabelText("strftime pattern") !== null);
    fireEvent.click(screen.getByRole("button", { name: /save/i }));

    const sawPut = () =>
      fetchMock.mock.calls.some(([u, i]) =>
        String(u).startsWith("/api/settings/datetime_formats") && ((i as RequestInit)?.method ?? "GET").toUpperCase() === "PUT",
      );
    await advanceUntil(sawPut);
    expect(sawPut()).toBe(true);
  });
});
