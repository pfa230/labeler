import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { FieldForm, type FormValue } from "./FieldForm";
import type { LayoutItem, TemplateDetail } from "../../api/types";

const single: TemplateDetail = {
  id: "t1",
  name: "Single",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  options: { variant: ["a", "b"] },
  layout: [{ type: "text", value: "{message}" }],
};

const sheet: TemplateDetail = {
  id: "s1",
  name: "Sheet",
  description: "",
  unit: "mm",
  dpi: 300,
  format: {
    type: "sheet",
    paper_width: 210,
    paper_height: 297,
    label_width: 60,
    label_height: 30,
    positions: [
      [0, 0],
      [60, 0],
      [120, 0],
    ],
  },
  layout: [{ type: "text", value: "{message}" }],
};

function renderForm(detail: TemplateDetail, value: FormValue, onChange = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const { unmount } = render(
    <QueryClientProvider client={qc}>
      <FieldForm detail={detail} value={value} onChange={onChange} />
    </QueryClientProvider>,
  );
  return Object.assign(onChange, { unmount });
}

// A TemplateDetail wrapping an arbitrary layout, for tests about control choice rather than format.
function detailWith(layout: LayoutItem[], options?: TemplateDetail["options"]): TemplateDetail {
  return { ...single, layout, options };
}

const singleValue: FormValue = { data: {}, option: { variant: "a" }, printer: undefined, startSlot: 0 };

describe("FieldForm", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify([]), { status: 200, headers: { "content-type": "application/json" } }),
      ),
    );
  });

  it("renders a text input per referenced field", async () => {
    renderForm(single, singleValue);
    expect(await screen.findByLabelText("message")).toBeInTheDocument();
  });

  it("renders an option select defaulting to the first value", async () => {
    renderForm(single, singleValue);
    const variant = (await screen.findByLabelText("variant")) as HTMLSelectElement;
    expect(variant.value).toBe("a");
    expect([...variant.options].map((o) => o.value)).toEqual(["a", "b"]);
  });

  it("fires onChange with the typed field value", async () => {
    const onChange = renderForm(single, singleValue);
    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ data: { message: "hello" } }),
    );
  });

  it("does not render a start-slot input for a single template", async () => {
    renderForm(single, singleValue);
    await screen.findByLabelText("message");
    expect(screen.queryByLabelText(/start slot/i)).not.toBeInTheDocument();
  });

  it("renders a start-slot number input for a sheet template", async () => {
    renderForm(sheet, { data: {}, option: {}, printer: undefined, startSlot: 0 });
    const slot = (await screen.findByLabelText(/start slot/i)) as HTMLInputElement;
    expect(slot.type).toBe("number");
  });

  it("renders a textarea for a multiline field and an input for a plain one", async () => {
    renderForm(
      detailWith([
        { type: "text", value: "{body}", multiline: true },
        { type: "text", value: "{title}" },
      ]),
      singleValue,
    );
    expect((await screen.findByLabelText("body")).tagName).toBe("TEXTAREA");
    expect(screen.getByLabelText("title").tagName).toBe("INPUT");
  });

  it("keeps the newline the user typed", async () => {
    const onChange = renderForm(
      detailWith([{ type: "text", value: "{body}", multiline: true }]),
      singleValue,
    );
    fireEvent.change(await screen.findByLabelText("body"), {
      target: { value: "one\ntwo" },
    });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ data: expect.objectContaining({ body: "one\ntwo" }) }),
    );
  });

  // An image field stays a file picker even when the same key is also multiline text: the picker is
  // the only control that can supply image data at all.
  it("keeps the file picker when a field is both an image and multiline text", async () => {
    // Not two siblings both named "logo": the backend rejects duplicate names in one items slice, so
    // that layout could never reach the form. A data-bound image plus a multiline text item
    // interpolating the same key is valid and produces the same collision.
    renderForm(
      detailWith([
        { type: "image", name: "logo" },
        { type: "text", value: "{logo}", multiline: true },
      ]),
      singleValue,
    );
    expect((await screen.findByLabelText("logo")).getAttribute("type")).toBe("file");
  });

  // The note is computed ungated, so it shows in the branch where the control is a plain input too:
  // that is the branch where typing under the other option silently truncates.
  it("flags a field that is also used by a single-line item, in either branch", async () => {
    const layout: LayoutItem[] = [
      { type: "container", option: { mode: "long" }, items: [{ type: "text", value: "{shared}", multiline: true }] },
      { type: "container", option: { mode: "short" }, items: [{ type: "text", value: "{shared}" }] },
    ];
    for (const mode of ["long", "short"]) {
      const { unmount } = renderForm(detailWith(layout, { mode: ["long", "short"] }), {
        ...singleValue,
        option: { mode },
      });
      const note = await screen.findByText(/shows only the first line/i);
      expect(await screen.findByLabelText("shared")).toHaveAttribute(
        "aria-describedby",
        note.getAttribute("id"),
      );
      unmount();
    }
  });

  /// Field names only have to be non-empty, so a name with a space would build an id containing a
  /// space — and aria-describedby is a whitespace-separated IDREFS list, which would silently point
  /// at nothing.
  it("associates the note even when the field name contains a space", async () => {
    const layout: LayoutItem[] = [
      { type: "text", value: "{customer name}", multiline: true },
      { type: "text", value: "{customer name}" },
    ];
    renderForm(detailWith(layout), singleValue);
    const note = await screen.findByText(/shows only the first line/i);
    const id = note.getAttribute("id") ?? "";
    expect(id).not.toContain(" ");
    expect(await screen.findByLabelText("customer name")).toHaveAttribute("aria-describedby", id);
  });

  it("does not flag a field used only by multiline items", async () => {
    renderForm(detailWith([{ type: "text", value: "{body}", multiline: true }]), singleValue);
    await screen.findByLabelText("body");
    expect(screen.queryByText(/shows only the first line/i)).not.toBeInTheDocument();
  });
});
