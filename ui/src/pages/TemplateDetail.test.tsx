import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route, useNavigate } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { TemplateDetail } from "./TemplateDetail";

const detail = {
  id: "brother_24mm_qr",
  name: "Brother 24mm Continuous Label",
  description: "Continuous label roll (24mm width)",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: { min: 10, max: 120 }, height: 24 },
  layout: [
    { type: "qr", value: "{code}" },
    { type: "text", value: "{message}" },
  ],
};

const source = "id: brother_24mm_qr\nname: Brother 24mm Continuous Label\n";

const other = { ...detail, id: "other_label", name: "Other Label" };
const otherSource = "id: other_label\nname: Other Label\n";

// What GET /source currently returns. A PUT overwrites it, so the stub models the server: the bytes
// the client sent are what a later read gets back.
let currentSource = source;

function stubFetch(deleteStatus = 204, putStatus = 200, sourceStatus = 200, slowOtherDetail = false) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (init?.method === "DELETE") {
        return deleteStatus === 204
          ? new Response(null, { status: 204 })
          : new Response(
              JSON.stringify({ error: { code: "RenderFailed", message: "delete failed" } }),
              { status: deleteStatus, headers: { "content-type": "application/json" } },
            );
      }
      if (init?.method === "PUT") {
        // Both branches persist. A 422 does not imply an unchanged file: the server writes before it
        // reloads, so a failed reload leaves the edit on disk (SPEC, PUT /templates/{id}).
        currentSource = String(init.body);
        return putStatus === 200
          ? new Response(JSON.stringify(detail), {
              status: 200,
              headers: { "content-type": "application/json" },
            })
          : new Response(
              JSON.stringify({
                error: { code: "TemplateInvalid", message: "layout[0].size: out of bounds" },
              }),
              { status: putStatus, headers: { "content-type": "application/json" } },
            );
      }
      if (url.endsWith("/api/templates/other_label/source")) {
        return new Response(otherSource, { status: 200, headers: { "content-type": "text/yaml" } });
      }
      if (url.endsWith("/api/templates/other_label")) {
        // Optionally slow: useTemplate keeps the previous detail as placeholder data, so this window
        // is exactly when the page shows template A's detail under template B's URL.
        if (slowOtherDetail) await new Promise((r) => setTimeout(r, 300));
        return new Response(JSON.stringify(other), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.endsWith("/api/templates/brother_24mm_qr/source")) {
        return sourceStatus === 200
          ? new Response(currentSource, { status: 200, headers: { "content-type": "text/yaml" } })
          : new Response("nope", { status: sourceStatus });
      }
      if (url.endsWith("/api/templates/brother_24mm_qr")) {
        return new Response(JSON.stringify(detail), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url.endsWith("/api/render/label")) {
        return new Response(new Blob(["x"]), {
          status: 200,
          headers: { "content-type": "image/png" },
        });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }),
  );
}

function GoToOther() {
  const navigate = useNavigate();
  return (
    <button type="button" onClick={() => navigate("/templates/other_label")}>
      go to other
    </button>
  );
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={["/templates/brother_24mm_qr"]}>
          <GoToOther />
          <Routes>
            <Route path="/templates/:id" element={<TemplateDetail />} />
            {/* The list is the app's index route; "/templates" is only a redirect to it. */}
            <Route path="/" element={<div>Labels list</div>} />
          </Routes>
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Template detail", () => {
  // Wrapped, not passed by reference: beforeEach hands the hook a test context, which would land in
  // stubFetch's deleteStatus parameter.
  beforeEach(() => {
    currentSource = source;
    stubFetch();
  });

  it("renders name, referenced fields, format badge, and a use-to-print link", async () => {
    renderPage();
    expect(await screen.findByText("Brother 24mm Continuous Label")).toBeInTheDocument();
    expect(screen.getByText("message")).toBeInTheDocument();
    expect(screen.getByText("code")).toBeInTheDocument();
    expect(screen.getByText("single")).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /use to print/i });
    expect(link).toHaveAttribute("href", "/print/brother_24mm_qr");
  });

  it("reveals the raw YAML source when toggled", async () => {
    renderPage();
    await screen.findByText("Brother 24mm Continuous Label");
    const toggle = await screen.findByText(/raw yaml/i);
    fireEvent.click(toggle);
    expect(await screen.findByText(/id: brother_24mm_qr/)).toBeInTheDocument();
  });

  it("deletes after confirming, then returns to the list", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    expect(await screen.findByText("Labels list")).toBeInTheDocument();
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    expect(
      calls.some(
        ([url, init]) =>
          String(url).endsWith("/api/templates/brother_24mm_qr") && init?.method === "DELETE",
      ),
    ).toBe(true);
    expect(await screen.findByText(/Deleted brother_24mm_qr/)).toBeInTheDocument();
  });

  it("cancelling sends no request", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls.some(([, init]) => init?.method === "DELETE")).toBe(false);
  });

  it("saves an edit through PUT and leaves edit mode", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Edited\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByRole("button", { name: "Edit" })).toBeInTheDocument();
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const put = calls.find(([, init]) => init?.method === "PUT");
    expect(String(put?.[1]?.body)).toContain("name: Edited");
  });

  it("keeps the draft and shows the server message when the save fails", async () => {
    stubFetch(204, 422);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Broken\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    // Twice by design: inline beside the textarea, and in the toast.
    expect(await screen.findAllByText(/layout\[0\]\.size: out of bounds/)).toHaveLength(2);
    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(
      "id: brother_24mm_qr\nname: Broken\n",
    );
  });

  it("re-entering edit after a save shows the saved text", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Saved\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));

    // Fails with a bare invalidation: the stale cached source wins the race and reseeds pre-save text.
    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(
      "id: brother_24mm_qr\nname: Saved\n",
    );
  });

  it("after a failed save, the next edit seeds from what is actually stored", async () => {
    // The persisted-but-422 case: the stub kept the submitted bytes (the write landed) and still
    // returned 422 (the reload failed), so the cached source is now wrong in the other direction.
    stubFetch(204, 422);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Persisted\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findAllByText(/out of bounds/);

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));

    // Content, not fetch counts: removeQueries refetches on the same re-render the error handler
    // causes, so a before/after tally races the behavior it is meant to check.
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(
        "id: brother_24mm_qr\nname: Persisted\n",
      ),
    );
  });

  it("cancelling a modified draft asks before discarding", async () => {
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "changed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));

    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue("changed");
  });

  it("drops an open draft when the route moves to another template", async () => {
    // The new detail is slow on purpose: `useTemplate` serves the old one as placeholder data
    // meanwhile, so keying the editor on detail.id would keep the previous template's draft mounted
    // and savable while the URL already points at the new one.
    stubFetch(204, 200, 200, true);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: /template yaml/i }), {
      target: { value: "id: brother_24mm_qr\nname: Mine\n" },
    });

    // React Router reuses the templates/:id component, so without a reset the draft would follow.
    fireEvent.click(screen.getByRole("button", { name: /go to other/i }));

    // Immediately, while the new detail is still loading: the draft must already be gone.
    expect(screen.queryByRole("textbox", { name: /template yaml/i })).not.toBeInTheDocument();
    expect(await screen.findByText("Other Label")).toBeInTheDocument();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    expect(screen.getByRole("textbox", { name: /template yaml/i })).toHaveValue(otherSource);
  });

  it("disables Edit and shows an error when the source cannot be loaded", async () => {
    stubFetch(204, 200, 500);
    renderPage();
    fireEvent.click(await screen.findByText(/raw yaml/i));
    expect(await screen.findByText(/could not load the template source/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit" })).toBeDisabled();
  });

  it("keeps the user on the page when the delete fails", async () => {
    stubFetch(500);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    expect(await screen.findByText(/delete failed/i)).toBeInTheDocument();
    expect(screen.queryByText("Labels list")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
  });
});
