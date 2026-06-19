import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { SettingsSection } from "./SettingsSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function stubFetch(initial: { value: number; is_default: boolean }) {
  const state = { ...initial };
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/settings/job_log_retention_days") && method === "PUT") {
      state.value = JSON.parse(init!.body as string).value as number;
      state.is_default = false;
      return json({ value: state.value, is_default: false });
    }
    if (url.startsWith("/api/settings/job_log_retention_days") && method === "DELETE") {
      state.value = 90;
      state.is_default = true;
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/settings")) {
      return json({ job_log_retention_days: { value: state.value, is_default: state.is_default } });
    }
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <SettingsSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string, method: string) =>
  [...fetchMock.mock.calls].reverse().find(([u, i]) => String(u).startsWith(path) && ((i as RequestInit)?.method ?? "GET").toUpperCase() === method);

describe("SettingsSection", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it("shows the resolved value flagged as default", async () => {
    fetchMock = stubFetch({ value: 90, is_default: true });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const input = (await screen.findByLabelText(/job_log_retention_days/i)) as HTMLInputElement;
    expect(input.value).toBe("90");
    expect(screen.getByText(/default/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /reset/i })).not.toBeInTheDocument();
  });

  it("saves an override via PUT", async () => {
    fetchMock = stubFetch({ value: 90, is_default: true });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const input = (await screen.findByLabelText(/job_log_retention_days/i)) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "30" } });
    fireEvent.click(screen.getByRole("button", { name: /save/i }));
    await waitFor(() => expect(lastCall("/api/settings/job_log_retention_days", "PUT")).toBeTruthy());
    const call = lastCall("/api/settings/job_log_retention_days", "PUT")!;
    expect(JSON.parse((call[1] as RequestInit).body as string)).toEqual({ value: 30 });
  });

  it("resets to default via DELETE when overridden", async () => {
    fetchMock = stubFetch({ value: 30, is_default: false });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    await screen.findByLabelText(/job_log_retention_days/i);
    fireEvent.click(screen.getByRole("button", { name: /reset/i }));
    await waitFor(() => expect(lastCall("/api/settings/job_log_retention_days", "DELETE")).toBeTruthy());
  });
});
