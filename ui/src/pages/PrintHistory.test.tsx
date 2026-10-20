import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { PrintHistory } from "./PrintHistory";
import type { JobRecord } from "../api/types";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const longError = "printer rejected job: paper jam on tray 2\nsecond line of detail";

const jobs: JobRecord[] = [
  {
    id: 5,
    timestamp: "2026-10-20T09:15:04Z",
    template_id: "local_tpl",
    printer_id: null,
    status: "ok",
    error: null,
    actor_id: "local",
    actor_username: null,
  },
  {
    id: 4,
    timestamp: "2026-10-20T09:15:03Z",
    template_id: "failed_tpl",
    printer_id: "p1",
    status: "failed",
    error: longError,
    actor_id: "u2",
    actor_username: "bob",
  },
  {
    id: 3,
    timestamp: "2026-10-20T09:15:02Z",
    template_id: "ok_tpl",
    printer_id: "p2",
    status: "ok",
    error: null,
    actor_id: "u1",
    actor_username: "alice",
  },
  {
    id: 2,
    timestamp: "2026-10-20T09:15:01Z",
    template_id: "old_tpl",
    printer_id: null,
    status: "ok",
    error: null,
    actor_id: "",
    actor_username: null,
  },
];

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <PrintHistory />
    </QueryClientProvider>,
  );
}

describe("Print history page", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("shows jobs in API order with actors, times, outcomes and full failure text", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => json(jobs)));
    renderPage();

    const table = await screen.findByRole("table");
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(4);

    for (const [index, job] of jobs.entries()) {
      expect(within(rows[index]).getByText(job.template_id)).toBeInTheDocument();
      const time = rows[index].querySelector("time");
      expect(time).toHaveAttribute("dateTime", job.timestamp);
      expect(time?.textContent).not.toBe("");
    }

    expect(within(rows[0]).getByText("local")).toBeInTheDocument();
    expect(within(rows[1]).getByText("bob")).toBeInTheDocument();
    expect(within(rows[2]).getByText("alice")).toBeInTheDocument();
    expect(within(rows[3]).getByText("unknown")).toBeInTheDocument();

    expect(within(rows[0]).getByText("Sent to printer")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Failed")).toBeInTheDocument();
    expect(within(rows[1]).getByText("p1")).toBeInTheDocument();
    expect(within(rows[0]).getByText("—")).toBeInTheDocument();

    expect(
      within(rows[1]).getByText((_, el) => el?.tagName === "PRE" && el.textContent === longError),
    ).toBeInTheDocument();
    expect(within(rows[0]).queryByText("Error detail")).not.toBeInTheDocument();
  });

  it("shows loading while the first read is pending", () => {
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(() => {})));
    renderPage();
    expect(screen.getByText(/loading print history/i)).toBeInTheDocument();
  });

  it("shows a distinct empty-history message", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => json([])));
    renderPage();
    expect(await screen.findByText(/no print jobs recorded yet/i)).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });

  it("shows the service error and reads again on Retry", async () => {
    let reads = 0;
    const fetchMock = vi.fn(async () => {
      reads += 1;
      return reads === 1
        ? json({ error: { code: "Internal", message: "Database unavailable" } }, 500)
        : json(jobs);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();

    expect(await screen.findByText("Database unavailable")).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /retry/i }));
    expect(await screen.findByRole("table")).toBeInTheDocument();
    await waitFor(() => expect(reads).toBe(2));
  });
});
