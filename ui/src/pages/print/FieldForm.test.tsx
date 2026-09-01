import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { FieldForm, type FormValue } from "./FieldForm";
import type { InputSpec, TemplateDetail } from "../../api/types";

const single: TemplateDetail = {
  id: "t1",
  name: "Single",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  inputs: {
    all: [{ name: "message", control: "text" }],
    default: [{ name: "message", control: "text" }],
  },
  variables: [],
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
  inputs: {
    all: [{ name: "message", control: "text" }],
    default: [{ name: "message", control: "text" }],
  },
  variables: [],
};

function renderForm(
  detail: TemplateDetail,
  value: FormValue,
  inputs?: InputSpec[],
  onChange = vi.fn(),
) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const { unmount } = render(
    <QueryClientProvider client={qc}>
      <FieldForm detail={detail} inputs={inputs} value={value} onChange={onChange} />
    </QueryClientProvider>,
  );
  return Object.assign(onChange, { unmount });
}

const singleValue: FormValue = { data: {}, deferred: {}, printer: undefined, startSlot: 0 };

// FieldForm updates functionally, so that a value read at render time cannot overwrite a later one.
// A test applies the last update to the value the form rendered with.
function lastUpdate(onChange: ReturnType<typeof vi.fn>, prev: FormValue): FormValue {
  const update = onChange.mock.calls.at(-1)![0] as (p: FormValue) => FormValue;
  return update(prev);
}

describe("FieldForm", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify([]), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("renders a text input for text control", async () => {
    renderForm(single, singleValue, [{ name: "message", control: "text" }]);
    expect(await screen.findByLabelText("message")).toBeInTheDocument();
  });

  it("renders a textarea for textarea control", async () => {
    renderForm(single, singleValue, [{ name: "notes", control: "textarea" }]);
    expect((await screen.findByLabelText("notes")).tagName).toBe("TEXTAREA");
  });

  it("renders number and integer inputs", async () => {
    const inputs: InputSpec[] = [
      { name: "count", control: "integer", min: 1, max: 100 },
      { name: "weight", control: "number", min: 0.1 },
    ];
    renderForm(single, singleValue, inputs);

    const count = (await screen.findByLabelText("count")) as HTMLInputElement;
    expect(count.type).toBe("number");
    expect(count.step).toBe("1");
    expect(count.min).toBe("1");
    expect(count.max).toBe("100");

    const weight = (await screen.findByLabelText("weight")) as HTMLInputElement;
    expect(weight.type).toBe("number");
  });

  it("renders slider control with range and number sync", async () => {
    const inputs: InputSpec[] = [
      { name: "opacity", control: "number", slider: true, min: 0, max: 100, default: 50 },
    ];
    const value = { ...singleValue, data: { opacity: 50 } };
    const onChange = renderForm(single, value, inputs);

    const slider = (await screen.findByLabelText("opacity")) as HTMLInputElement;
    expect(slider.type).toBe("range");
    expect(slider.min).toBe("0");
    expect(slider.max).toBe("100");
    expect(slider.step).toBe("any");

    fireEvent.change(slider, { target: { value: "75" } });
    expect(lastUpdate(onChange, value)).toEqual(
      expect.objectContaining({ data: { opacity: 75 } }),
    );
  });

  it("renders select control with options", async () => {
    const inputs: InputSpec[] = [
      { name: "flavor", control: "select", values: ["vanilla", "chocolate"], default: "vanilla" },
    ];
    renderForm(single, { ...singleValue, data: { flavor: "vanilla" } }, inputs);

    const select = (await screen.findByLabelText("flavor")) as HTMLSelectElement;
    expect(select.tagName).toBe("SELECT");
    expect(select.value).toBe("vanilla");
    expect([...select.options].map((o) => o.value)).toEqual(["vanilla", "chocolate"]);
  });

  it("renders checkbox control", async () => {
    const inputs: InputSpec[] = [
      { name: "active", control: "checkbox", default: true },
    ];
    const value = { ...singleValue, data: { active: true } };
    const onChange = renderForm(single, value, inputs);

    const checkbox = (await screen.findByLabelText("active")) as HTMLInputElement;
    expect(checkbox.type).toBe("checkbox");
    expect(checkbox.checked).toBe(true);

    fireEvent.click(checkbox);
    expect(lastUpdate(onChange, value)).toEqual(
      expect.objectContaining({ data: { active: false } }),
    );
  });

  it("renders datetime and date inputs", async () => {
    const inputs: InputSpec[] = [
      { name: "created_at", control: "datetime" },
      { name: "ship_date", control: "date" },
    ];
    renderForm(single, singleValue, inputs);

    const dt = (await screen.findByLabelText("created_at")) as HTMLInputElement;
    expect(dt.type).toBe("datetime-local");

    const d = (await screen.findByLabelText("ship_date")) as HTMLInputElement;
    expect(d.type).toBe("date");
  });

  it("renders file picker for image control", async () => {
    const inputs: InputSpec[] = [
      { name: "logo", control: "image" },
    ];
    renderForm(single, singleValue, inputs);

    const picker = (await screen.findByLabelText("logo")) as HTMLInputElement;
    expect(picker.type).toBe("file");
  });

  it("does not render a start-slot input for a single template", async () => {
    renderForm(single, singleValue);
    await screen.findByLabelText("message");
    expect(screen.queryByLabelText(/start slot/i)).not.toBeInTheDocument();
  });

  it("renders a start-slot number input for a sheet template", async () => {
    renderForm(sheet, { data: {}, deferred: {}, printer: undefined, startSlot: 0 });
    const slot = (await screen.findByLabelText(/start slot/i)) as HTMLInputElement;
    expect(slot.type).toBe("number");
  });

  it("fires onChange with typed field value", async () => {
    const onChange = renderForm(single, singleValue, [{ name: "message", control: "text" }]);
    fireEvent.change(await screen.findByLabelText("message"), { target: { value: "hello" } });
    expect(lastUpdate(onChange, singleValue)).toEqual(
      expect.objectContaining({ data: { message: "hello" } }),
    );
  });

  it("renders a checked Use default checkbox naming the published default and disables the control", async () => {
    const inputs: InputSpec[] = [{ name: "title", control: "text", default: "Untitled" }];
    renderForm(single, { ...singleValue, data: { title: "Untitled" }, deferred: { title: true } }, inputs);

    const checkbox = (await screen.findByRole("checkbox", {
      name: "Use default for title",
    })) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
    expect(screen.getByText(/Use default/)).toHaveTextContent("Untitled");
    expect(screen.getByRole("textbox", { name: "title" })).toBeDisabled();
  });

  it("renders no Use default checkbox for an entry publishing no default", async () => {
    renderForm(single, singleValue, [{ name: "message", control: "text" }]);

    await screen.findByLabelText("message");
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(screen.queryByText(/Use default/)).not.toBeInTheDocument();
  });

  it("clears deferral without touching the value, and restores the default on re-checking", async () => {
    const inputs: InputSpec[] = [{ name: "title", control: "text", default: "Untitled" }];
    const deferredValue: FormValue = { ...singleValue, data: { title: "Untitled" }, deferred: { title: true } };
    const onChange = renderForm(single, deferredValue, inputs);

    fireEvent.click(await screen.findByRole("checkbox", { name: "Use default for title" }));
    const cleared = lastUpdate(onChange, deferredValue);
    expect(cleared.deferred).toEqual({ title: false });
    expect(cleared.data).toEqual({ title: "Untitled" });
    onChange.unmount();

    // Re-checking after an edit discards it, whatever the control then held.
    const editedValue: FormValue = { ...singleValue, data: { title: "Kitchen" }, deferred: { title: false } };
    const onChange2 = renderForm(single, editedValue, inputs);
    expect(screen.getByRole("textbox", { name: "title" })).not.toBeDisabled();

    fireEvent.click(screen.getByRole("checkbox", { name: "Use default for title" }));
    const recheck = lastUpdate(onChange2, editedValue);
    expect(recheck.deferred).toEqual({ title: true });
    expect(recheck.data).toEqual({ title: "Untitled" });
  });

  it("gives two entries sharing a description and a default distinct accessible names", async () => {
    const inputs: InputSpec[] = [
      { name: "title", description: "Line", control: "text", default: "Untitled" },
      { name: "subtitle", description: "Line", control: "text", default: "Untitled" },
    ];
    renderForm(single, { ...singleValue, deferred: { title: true, subtitle: true } }, inputs);

    expect(await screen.findByRole("checkbox", { name: "Use default for title" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Use default for subtitle" })).toBeInTheDocument();
  });

  it("renders no Use default checkbox and surfaces error message for an entry carrying default_error", async () => {
    const inputs: InputSpec[] = [
      {
        name: "site",
        control: "text",
        required: true,
        default_error: {
          reason: "param_default_unresolvable",
          message: "variable 'site' not found",
          token: "vars.site",
        },
      },
    ];
    renderForm(single, singleValue, inputs);

    expect(await screen.findByText("variable 'site' not found")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /use default/i })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "site" })).toBeEnabled();
  });
});
