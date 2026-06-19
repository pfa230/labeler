import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { VariablesSection } from "./VariablesSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

// Stateful stub: PUT mutates `settings` and GET returns a fresh copy, so an invalidate+refetch reflects
// the saved value (which is what makes a saved row stop being dirty).
function stubFetch(settings: Record<string, string>) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/variables/")) {
      const key = decodeURIComponent(url.slice("/api/variables/".length));
      const value = JSON.parse(init!.body as string).value as string;
      settings[key] = value;
      return json({ value });
    }
    if (url.startsWith("/api/variables")) return json({ ...settings });
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <VariablesSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string) => [...fetchMock.mock.calls].reverse().find(([u]) => String(u).startsWith(path));
const settingsGets = () => fetchMock.mock.calls.filter(([u]) => String(u) === "/api/variables").length;

describe("VariablesSection", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("shows existing variables and the suggested qr_base_url row when absent", async () => {
    fetchMock = stubFetch({ company: "Acme" });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    expect(await screen.findByLabelText("company")).toHaveValue("Acme");
    // qr_base_url is suggested even though it is not stored
    const qr = (await screen.findByLabelText("qr_base_url")) as HTMLInputElement;
    expect(qr.value).toBe("");
    expect(screen.getByText(/suggested/i)).toBeInTheDocument();
  });

  it("saves an edited variable via PUT /variables/{key}", async () => {
    fetchMock = stubFetch({ company: "Acme" });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const input = (await screen.findByLabelText("company")) as HTMLInputElement;
    const getsBefore = settingsGets();
    fireEvent.change(input, { target: { value: "Globex" } });
    fireEvent.click(screen.getByRole("button", { name: /save company/i }));
    await waitFor(() => expect(lastCall("/api/variables/company")).toBeTruthy());
    const call = lastCall("/api/variables/company")!;
    expect((call[1] as RequestInit).method).toBe("PUT");
    expect(JSON.parse((call[1] as RequestInit).body as string)).toEqual({ value: "Globex" });
    // Wait for the post-save refetch (so the mutation has settled and isPending is false), then the row
    // is disabled because draft === the refetched value (clean), not merely because it is pending.
    await waitFor(() => expect(settingsGets()).toBeGreaterThan(getsBefore));
    expect(screen.getByRole("button", { name: /save company/i })).toBeDisabled();
  });

  it("adds a custom variable and rejects an invalid key client-side", async () => {
    fetchMock = stubFetch({});
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.change(await screen.findByLabelText(/new variable key/i), { target: { value: "bad key" } });
    fireEvent.change(screen.getByLabelText(/new variable value/i), { target: { value: "x" } });
    fireEvent.click(screen.getByRole("button", { name: /add variable/i }));
    expect(await screen.findByText(/must be non-empty and contain only/i)).toBeInTheDocument();
    expect([...fetchMock.mock.calls].some(([u]) => String(u).startsWith("/api/variables/"))).toBe(false);

    fireEvent.change(screen.getByLabelText(/new variable key/i), { target: { value: "label_dpi" } });
    fireEvent.click(screen.getByRole("button", { name: /add variable/i }));
    await waitFor(() => expect(lastCall("/api/variables/label_dpi")).toBeTruthy());
  });

  it("rejects adding a key that already exists (would strand its row)", async () => {
    fetchMock = stubFetch({ qr_base_url: "https://x" });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    await screen.findByLabelText("qr_base_url");
    fireEvent.change(screen.getByLabelText(/new variable key/i), { target: { value: "qr_base_url" } });
    fireEvent.change(screen.getByLabelText(/new variable value/i), { target: { value: "y" } });
    fireEvent.click(screen.getByRole("button", { name: /add variable/i }));
    expect(await screen.findByText(/already exists/i)).toBeInTheDocument();
    expect([...fetchMock.mock.calls].some(([u]) => String(u).startsWith("/api/variables/qr_base_url"))).toBe(false);
  });
});
