import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { FieldForm, type FormValue } from "./FieldForm";
import type { TemplateDetail } from "../../api/types";

const single: TemplateDetail = {
  id: "t1",
  name: "Single",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  options: { variant: ["a", "b"] },
  layout: [{ type: "text", name: "message" }],
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
  layout: [{ type: "text", name: "message" }],
};

function renderForm(detail: TemplateDetail, value: FormValue, onChange = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <FieldForm detail={detail} value={value} onChange={onChange} />
    </QueryClientProvider>,
  );
  return onChange;
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
});
