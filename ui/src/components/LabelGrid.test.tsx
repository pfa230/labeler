import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { LabelGrid, type LabelGridProps } from "./LabelGrid";
import type { LabelGridRow } from "../lib/labelGrid";
import { pruneDataForSubmit } from "../lib/labelInputs";
import type { InputSpec } from "../api/types";

const selectionBaseProps = {
  rows: [
    { id: "r1", origin: "csv" as const, data: { title: "a" }, validation: {} },
    { id: "r2", origin: "csv" as const, data: { title: "b" }, validation: {} },
  ] satisfies LabelGridRow[],
  fields: ["title"],
  onRowsChange: vi.fn(),
  onDuplicate: vi.fn(),
  onRemove: vi.fn(),
};

function sampleRows(): LabelGridRow[] {
  return [
    { id: "a", origin: "csv", data: { sku: "1", notes: "first" }, validation: {} },
    {
      id: "b",
      origin: "csv",
      data: { sku: "2", notes: "second" },
      validation: {},
      annotation: { status: "failed", message: "boom" },
    },
  ];
}

const defaultProps = {
  fields: ["sku", "notes"],
};

describe("LabelGrid selection and row actions", () => {
  it("calls onSelectRow when a row's preview radio is clicked", () => {
    const onSelectRow = vi.fn();
    render(<LabelGrid {...selectionBaseProps} selectedRowId="r1" onSelectRow={onSelectRow} />);
    fireEvent.click(screen.getByLabelText("preview row 2"));
    expect(onSelectRow).toHaveBeenCalledWith("r2");
  });

  it("renders no preview radios when onSelectRow is absent", () => {
    render(<LabelGrid {...selectionBaseProps} />);
    expect(screen.queryByLabelText("preview row 1")).toBeNull();
  });

  it("shows the annotation message for a failed row and ok for success", () => {
    const rowsWithStatus: LabelGridRow[] = [
      { id: "a", origin: "csv", data: { sku: "1" }, validation: {}, annotation: { status: "ok" } },
      { id: "b", origin: "csv", data: { sku: "2" }, validation: {}, annotation: { status: "failed", message: "boom" } },
    ];
    render(
      <LabelGrid
        rows={rowsWithStatus}
        fields={["sku"]}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );
    expect(screen.getByText("ok")).toBeInTheDocument();
    expect(screen.getByText(/failed: boom/)).toBeInTheDocument();
  });

  it("calls onDuplicate and onRemove with the row id", () => {
    const onDuplicate = vi.fn();
    const onRemove = vi.fn();
    render(
      <LabelGrid
        rows={sampleRows()}
        {...defaultProps}
        onRowsChange={() => {}}
        onDuplicate={onDuplicate}
        onRemove={onRemove}
      />,
    );
    fireEvent.click(screen.getAllByRole("button", { name: /duplicate/i })[0]);
    fireEvent.click(screen.getAllByRole("button", { name: /remove/i })[0]);
    expect(onDuplicate).toHaveBeenCalledWith("a");
    expect(onRemove).toHaveBeenCalledWith("a");
  });

  it("renders no operable controls and disables row actions when disabled", () => {
    const onRowsChange = vi.fn();
    render(
      <LabelGrid
        rows={sampleRows()}
        {...defaultProps}
        disabled
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );
    // At rest while disabled, cells render plain text spans
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("first")).toBeInTheDocument();
    expect(screen.queryByLabelText("edit sku")).toBeNull();
    expect(screen.queryByLabelText("edit notes")).toBeNull();
    expect(screen.getAllByRole("button", { name: /duplicate/i })[0]).toBeDisabled();
    expect(screen.getAllByRole("button", { name: /remove/i })[0]).toBeDisabled();
  });
});

describe("4.1 Always-on rendering at rest across control types", () => {
  it("renders text, textarea, select, checkbox, integer, number, date, datetime directly at rest without gesture", () => {
    const row: LabelGridRow = {
      id: "r1",
      origin: "csv",
      data: {
        textField: "hello",
        textareaField: "multiline note",
        selectField: "opt1",
        checkboxField: "true",
        intField: "42",
        numField: "3.14",
        dateField: "2026-09-12",
        datetimeField: "2026-09-12T10:30",
        listField: ["a", "b"],
        imageField: "data:image/png;base64,abc",
      },
      validation: {},
    };

    const specs: Record<string, InputSpec> = {
      textField: { name: "textField", control: "text" },
      textareaField: { name: "textareaField", control: "textarea" },
      selectField: { name: "selectField", control: "select", values: ["opt1", "opt2"] },
      checkboxField: { name: "checkboxField", control: "checkbox" },
      intField: { name: "intField", control: "integer", min: 1, max: 100 },
      numField: { name: "numField", control: "number" },
      dateField: { name: "dateField", control: "date" },
      datetimeField: { name: "datetimeField", control: "datetime" },
      listField: { name: "listField", control: "list" },
      imageField: { name: "imageField", control: "image" },
    };

    const fields = Object.keys(specs);
    const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => specs[f];

    render(
      <LabelGrid
        rows={[row]}
        fields={fields}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    // text: single-line input
    const textInput = screen.getByLabelText("edit textField") as HTMLInputElement;
    expect(textInput.tagName).toBe("INPUT");
    expect(textInput.type).toBe("text");
    expect(textInput.value).toBe("hello");

    // textarea: textarea element
    const textarea = screen.getByLabelText("edit textareaField") as HTMLTextAreaElement;
    expect(textarea.tagName).toBe("TEXTAREA");
    expect(textarea.value).toBe("multiline note");

    // select: select element with declared options + (none)
    const select = screen.getByLabelText("edit selectField") as HTMLSelectElement;
    expect(select.tagName).toBe("SELECT");
    expect(select.value).toBe("opt1");
    const optionValues = Array.from(select.querySelectorAll("option")).map((o) => o.value);
    expect(optionValues).toEqual(["", "opt1", "opt2"]);

    // checkbox: checkbox input
    const checkbox = screen.getByLabelText("edit checkboxField") as HTMLInputElement;
    expect(checkbox.tagName).toBe("INPUT");
    expect(checkbox.type).toBe("checkbox");
    expect(checkbox.checked).toBe(true);

    // integer: input type=number step=1
    const intInput = screen.getByLabelText("edit intField") as HTMLInputElement;
    expect(intInput.tagName).toBe("INPUT");
    expect(intInput.type).toBe("number");
    expect(intInput.step).toBe("1");
    expect(intInput.min).toBe("1");
    expect(intInput.max).toBe("100");
    expect(intInput.value).toBe("42");

    // number: input type=number step=any
    const numInput = screen.getByLabelText("edit numField") as HTMLInputElement;
    expect(numInput.tagName).toBe("INPUT");
    expect(numInput.type).toBe("number");
    expect(numInput.step).toBe("any");
    expect(numInput.value).toBe("3.14");

    // date: input type=date
    const dateInput = screen.getByLabelText("edit dateField") as HTMLInputElement;
    expect(dateInput.tagName).toBe("INPUT");
    expect(dateInput.type).toBe("date");
    expect(dateInput.value).toBe("2026-09-12");

    // datetime: input type=datetime-local
    const datetimeInput = screen.getByLabelText("edit datetimeField") as HTMLInputElement;
    expect(datetimeInput.tagName).toBe("INPUT");
    expect(datetimeInput.type).toBe("datetime-local");
    expect(datetimeInput.value).toBe("2026-09-12T10:30");

    // list: plain text, no operable control
    expect(screen.getByText("a, b")).toBeInTheDocument();
    expect(screen.queryByLabelText("edit listField")).toBeNull();

    // image: plain text marker "image", no operable control
    expect(screen.getByText("image")).toBeInTheDocument();
    expect(screen.queryByLabelText("edit imageField")).toBeNull();
  });

  it("renders list and disabled cells as plain text without controls", () => {
    const row: LabelGridRow = {
      id: "r1",
      origin: "csv",
      data: { tags: ["RED", "BLUE"], title: "Widget" },
      validation: {},
    };
    const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => {
      if (f === "tags") return { name: "tags", control: "list" };
      return { name: "title", control: "text" };
    };

    const { rerender } = render(
      <LabelGrid
        rows={[row]}
        fields={["tags", "title"]}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(screen.getByText("RED, BLUE")).toBeInTheDocument();
    expect(screen.queryByLabelText("edit tags")).toBeNull();
    expect(screen.getByLabelText("edit title")).toBeInTheDocument();

    // With disabled=true, even title becomes plain text
    rerender(
      <LabelGrid
        rows={[row]}
        fields={["tags", "title"]}
        cellInput={cellInput}
        disabled
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(screen.getByText("RED, BLUE")).toBeInTheDocument();
    expect(screen.getByText("Widget")).toBeInTheDocument();
    expect(screen.queryByLabelText("edit tags")).toBeNull();
    expect(screen.queryByLabelText("edit title")).toBeNull();
  });
});

describe("4.2 Invalid cells repairability", () => {
  it("eligible invalid cell renders operable control alongside visual ⚠ error marker", () => {
    const invalidRow: LabelGridRow = {
      id: "r1",
      origin: "csv",
      data: { name: "", size: "" },
      validation: { field: { name: "required", size: "required" } },
    };
    const specs: Record<string, InputSpec> = {
      name: { name: "name", control: "text", required: true },
      size: { name: "size", control: "select", values: ["S", "M", "L"], required: true },
    };

    render(
      <LabelGrid
        rows={[invalidRow]}
        fields={["name", "size"]}
        cellInput={(_r, f) => specs[f]}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    // Both operable controls exist
    const nameInput = screen.getByLabelText("edit name");
    const sizeSelect = screen.getByLabelText("edit size");
    expect(nameInput).toBeInTheDocument();
    expect(sizeSelect).toBeInTheDocument();

    // Alongside visual ⚠ error markers
    expect(screen.getByLabelText("name required")).toHaveTextContent("⚠ required");
    expect(screen.getByLabelText("size required")).toHaveTextContent("⚠ required");
  });

  it("initially invalid text cell can be filled to commit a valid value", async () => {
    let currentRows: LabelGridRow[] = [
      { id: "r1", origin: "csv", data: { name: "" }, validation: { field: { name: "required" } } },
    ];
    const onRowsChange = vi.fn<LabelGridProps["onRowsChange"]>((next) => {
      currentRows = next;
    });

    render(
      <LabelGrid
        rows={currentRows}
        fields={["name"]}
        cellInput={(_r, f) => ({ name: f, control: "text" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const nameInput = screen.getByLabelText("edit name");
    expect(screen.getByLabelText("name required")).toBeInTheDocument();

    fireEvent.change(nameInput, { target: { value: "Widget A" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());

    expect(currentRows[0].data.name).toBe("Widget A");
    expect(onRowsChange.mock.calls.at(-1)![1]).toEqual({ indexes: [0] });
  });

  it("clearing then refilling a required text restores the row and retains operable control throughout", async () => {
    let currentRows: LabelGridRow[] = [
      { id: "r1", origin: "csv", data: { name: "Initial" }, validation: {} },
    ];
    const onRowsChange = vi.fn((next: LabelGridRow[]) => {
      currentRows = next;
    });

    const { rerender } = render(
      <LabelGrid
        rows={currentRows}
        fields={["name"]}
        cellInput={(_r, f) => ({ name: f, control: "text", required: true })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const nameInput = screen.getByLabelText("edit name") as HTMLInputElement;
    expect(nameInput.value).toBe("Initial");

    // Clear the field
    fireEvent.change(nameInput, { target: { value: "" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));
    expect(currentRows[0].data.name).toBe("");

    // Re-render with validation error
    rerender(
      <LabelGrid
        rows={[{ id: "r1", origin: "csv", data: { name: "" }, validation: { field: { name: "required" } } }]}
        fields={["name"]}
        cellInput={(_r, f) => ({ name: f, control: "text", required: true })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    // Control is still operable and error marker is visible
    const clearedInput = screen.getByLabelText("edit name") as HTMLInputElement;
    expect(clearedInput).toBeInTheDocument();
    expect(screen.getByLabelText("name required")).toHaveTextContent("⚠ required");

    // Refill the field
    fireEvent.change(clearedInput, { target: { value: "Restored" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(2));
    expect(currentRows[0].data.name).toBe("Restored");
  });
});

describe("4.3 Focus, pointer, and keyboard interactions", () => {
  it("initial focus lands on freshly mounted grid's first editable cell without preview column", () => {
    render(
      <LabelGrid
        rows={sampleRows()}
        {...defaultProps}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const firstEditable = screen.getAllByLabelText("edit sku")[0];
    expect(document.activeElement).toBe(firstEditable);
  });

  it("initial focus does NOT land on editable cell when preview column or disabled is present", () => {
    const { unmount } = render(
      <LabelGrid
        rows={sampleRows()}
        {...defaultProps}
        selectedRowId="a"
        onSelectRow={() => {}}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const firstEditable = screen.getAllByLabelText("edit sku")[0];
    expect(document.activeElement).not.toBe(firstEditable);
    unmount();

    render(
      <LabelGrid
        rows={sampleRows()}
        {...defaultProps}
        disabled
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );
    expect(screen.queryByLabelText("edit sku")).toBeNull();
  });

  it("click inside textarea and select stops propagation so vendor wrapper does not steal focus", () => {
    const row: LabelGridRow = {
      id: "r1",
      origin: "csv",
      data: { note: "text", choice: "a" },
      validation: {},
    };
    const cellInput = (_r: LabelGridRow, f: string): InputSpec => ({
      name: f,
      control: f === "note" ? "textarea" : "select",
      values: f === "choice" ? ["a", "b"] : undefined,
    });

    render(
      <LabelGrid
        rows={[row]}
        fields={["note", "choice"]}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const textarea = screen.getByLabelText("edit note");
    const mousedownEvent = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    const stopPropSpy = vi.spyOn(mousedownEvent, "stopPropagation");
    textarea.dispatchEvent(mousedownEvent);
    expect(stopPropSpy).toHaveBeenCalled();

    textarea.focus();
    expect(document.activeElement).toBe(textarea);

    const select = screen.getByLabelText("edit choice");
    const selectClickEvent = new MouseEvent("click", { bubbles: true, cancelable: true });
    const selectStopSpy = vi.spyOn(selectClickEvent, "stopPropagation");
    select.dispatchEvent(selectClickEvent);
    expect(selectStopSpy).toHaveBeenCalled();

    select.focus();
    expect(document.activeElement).toBe(select);
  });

  it("Arrow keys inside controls stop propagation to prevent grid cell navigation", () => {
    const row: LabelGridRow = {
      id: "r1",
      origin: "csv",
      data: { note: "line1\nline2", count: "5", choice: "x" },
      validation: {},
    };
    const cellInput = (_r: LabelGridRow, f: string): InputSpec => {
      if (f === "note") return { name: f, control: "textarea" };
      if (f === "count") return { name: f, control: "integer" };
      return { name: f, control: "select", values: ["x", "y"] };
    };

    render(
      <LabelGrid
        rows={[row]}
        fields={["note", "count", "choice"]}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const textarea = screen.getByLabelText("edit note");
    for (const key of ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"]) {
      const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      const spy = vi.spyOn(e, "stopPropagation");
      textarea.dispatchEvent(e);
      expect(spy).toHaveBeenCalled();
    }

    const numInput = screen.getByLabelText("edit count");
    for (const key of ["ArrowUp", "ArrowDown"]) {
      const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      const spy = vi.spyOn(e, "stopPropagation");
      numInput.dispatchEvent(e);
      expect(spy).toHaveBeenCalled();
    }

    const select = screen.getByLabelText("edit choice");
    for (const key of ["ArrowUp", "ArrowDown"]) {
      const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      const spy = vi.spyOn(e, "stopPropagation");
      select.dispatchEvent(e);
      expect(spy).toHaveBeenCalled();
    }
  });
});

describe("4.4 Multiline, unrepresentable presentations, and aria-readonly", () => {
  it("editable text holding multiline value shows first line with +N and submits newline unaltered until explicit edit", async () => {
    let currentRows: LabelGridRow[] = [
      { id: "r1", origin: "csv", data: { title: "first\nsecond\nthird" }, validation: {} },
    ];
    const onRowsChange = vi.fn((next: LabelGridRow[]) => {
      currentRows = next;
    });

    const { rerender } = render(
      <LabelGrid
        rows={currentRows}
        fields={["title"]}
        cellInput={(_r, f) => ({ name: f, control: "text" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    // Input shows first line, adjacent adornment shows +2, and wrapper title has full stored value
    const input = screen.getByLabelText("edit title") as HTMLInputElement;
    expect(input.value).toBe("first");
    expect(screen.getByText("+2")).toBeInTheDocument();
    const cellWrapper = input.closest(".flex")!;
    expect(cellWrapper).toHaveAttribute("title", "first\nsecond\nthird");

    // Leaving without typing does not fire onRowsChange; full multiline value is preserved
    input.focus();
    input.blur();
    expect(onRowsChange).not.toHaveBeenCalled();
    expect(currentRows[0].data.title).toBe("first\nsecond\nthird");

    // Explicit edit replaces stored value
    fireEvent.change(input, { target: { value: "edited title" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));
    expect(currentRows[0].data.title).toBe("edited title");

    // Re-render with new value: +2 adornment is gone
    rerender(
      <LabelGrid
        rows={currentRows}
        fields={["title"]}
        cellInput={(_r, f) => ({ name: f, control: "text" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );
    expect(screen.queryByText("+2")).toBeNull();
    expect(screen.getByDisplayValue("edited title")).toBeInTheDocument();
  });

  it("date holding offset-bearing RFC 3339 instant shows operable empty date control plus text adornment and preserves value until explicit pick", async () => {
    let currentRows: LabelGridRow[] = [
      { id: "r1", origin: "csv", data: { shipDate: "2026-09-01T12:00:00Z" }, validation: {} },
    ];
    const onRowsChange = vi.fn((next: LabelGridRow[]) => {
      currentRows = next;
    });

    const { rerender } = render(
      <LabelGrid
        rows={currentRows}
        fields={["shipDate"]}
        cellInput={(_r, f) => ({ name: f, control: "date" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const dateInput = screen.getByLabelText("edit shipDate") as HTMLInputElement;
    expect(dateInput.type).toBe("date");
    expect(dateInput.value).toBe(""); // unrepresentable date control shows empty

    // Visible text adornment shows raw value
    expect(screen.getByText("2026-09-01T12:00:00Z")).toBeInTheDocument();
    expect(dateInput).toHaveAttribute("aria-describedby", "adorn-r1-shipDate");

    // Focusing without explicit change preserves raw value
    dateInput.focus();
    dateInput.blur();
    expect(onRowsChange).not.toHaveBeenCalled();
    expect(currentRows[0].data.shipDate).toBe("2026-09-01T12:00:00Z");

    // Explicit pick replaces value and clears adornment
    fireEvent.change(dateInput, { target: { value: "2026-09-05" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));
    expect(currentRows[0].data.shipDate).toBe("2026-09-05");

    rerender(
      <LabelGrid
        rows={currentRows}
        fields={["shipDate"]}
        cellInput={(_r, f) => ({ name: f, control: "date" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );
    expect(screen.queryByText("2026-09-01T12:00:00Z")).toBeNull();
    expect(screen.getByDisplayValue("2026-09-05")).toBeInTheDocument();
  });

  it("integer holding non-numeric shows empty control plus text adornment with explicit change boundary", async () => {
    let currentRows: LabelGridRow[] = [
      { id: "r1", origin: "csv", data: { qty: "abc" }, validation: {} },
    ];
    const onRowsChange = vi.fn((next: LabelGridRow[]) => {
      currentRows = next;
    });

    const { rerender } = render(
      <LabelGrid
        rows={currentRows}
        fields={["qty"]}
        cellInput={(_r, f) => ({ name: f, control: "integer" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const numInput = screen.getByLabelText("edit qty") as HTMLInputElement;
    expect(numInput.value).toBe("");
    expect(screen.getByText("abc")).toBeInTheDocument();
    expect(numInput).toHaveAttribute("aria-describedby", "adorn-r1-qty");

    // Focus and blur without edit preserves raw value
    numInput.focus();
    numInput.blur();
    expect(onRowsChange).not.toHaveBeenCalled();
    expect(currentRows[0].data.qty).toBe("abc");

    // Explicit typing in number commits new number
    fireEvent.change(numInput, { target: { value: "10" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));
    expect(currentRows[0].data.qty).toBe("10");

    rerender(
      <LabelGrid
        rows={currentRows}
        fields={["qty"]}
        cellInput={(_r, f) => ({ name: f, control: "integer" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );
    expect(screen.queryByText("abc")).toBeNull();
    expect(screen.getByDisplayValue("10")).toBeInTheDocument();
  });

  it("checkbox holding malformed value shows unset control plus text adornment with explicit change boundary", async () => {
    let currentRows: LabelGridRow[] = [
      { id: "r1", origin: "csv", data: { flag: "maybe" }, validation: {} },
    ];
    const onRowsChange = vi.fn((next: LabelGridRow[]) => {
      currentRows = next;
    });

    const { rerender } = render(
      <LabelGrid
        rows={currentRows}
        fields={["flag"]}
        cellInput={(_r, f) => ({ name: f, control: "checkbox" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const checkbox = screen.getByLabelText("edit flag") as HTMLInputElement;
    expect(checkbox.checked).toBe(false);
    expect(checkbox.indeterminate).toBe(true);
    expect(screen.getByText("maybe")).toBeInTheDocument();
    expect(checkbox).toHaveAttribute("aria-describedby", "adorn-r1-flag");

    // Value preserved without explicit change
    expect(onRowsChange).not.toHaveBeenCalled();
    expect(currentRows[0].data.flag).toBe("maybe");

    // Explicit click on checkbox transitions to checked ("true")
    fireEvent.click(checkbox);
    await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));
    expect(currentRows[0].data.flag).toBe("true");

    rerender(
      <LabelGrid
        rows={currentRows}
        fields={["flag"]}
        cellInput={(_r, f) => ({ name: f, control: "checkbox" })}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );
    expect(screen.queryByText("maybe")).toBeNull();
    const updatedCheckbox = screen.getByLabelText("edit flag") as HTMLInputElement;
    expect(updatedCheckbox.checked).toBe(true);
    expect(updatedCheckbox.indeterminate).toBe(false);
  });

  it("synchronizes aria-readonly on cell wrapper: false for operable, true for missing/list/image/disabled, updating on transitions without remount", () => {
    const row: LabelGridRow = {
      id: "r1",
      origin: "csv",
      data: {
        operable: "test",
        missingEntry: "none",
        listCol: ["a", "b"],
        imageCol: "data:img",
      },
      validation: {},
    };

    const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => {
      if (f === "operable") return { name: f, control: "text" };
      if (f === "missingEntry") return undefined;
      if (f === "listCol") return { name: f, control: "list" };
      if (f === "imageCol") return { name: f, control: "image" };
      return undefined;
    };

    const { container, rerender } = render(
      <LabelGrid
        rows={[row]}
        fields={["operable", "missingEntry", "listCol", "imageCol"]}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const operableCell = container.querySelector('[data-col-id$="data:operable"]')!;
    const missingCell = container.querySelector('[data-col-id$="data:missingEntry"]')!;
    const listCell = container.querySelector('[data-col-id$="data:listCol"]')!;
    const imageCell = container.querySelector('[data-col-id$="data:imageCol"]')!;

    expect(operableCell).toHaveAttribute("aria-readonly", "false");
    expect(missingCell).toHaveAttribute("aria-readonly", "true");
    expect(listCell).toHaveAttribute("aria-readonly", "true");
    expect(imageCell).toHaveAttribute("aria-readonly", "true");

    // Transition to disabled=true updates aria-readonly to true for operable without remount
    rerender(
      <LabelGrid
        rows={[row]}
        fields={["operable", "missingEntry", "listCol", "imageCol"]}
        cellInput={cellInput}
        disabled
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(operableCell).toHaveAttribute("aria-readonly", "true");

    // Transition back to disabled=false restores aria-readonly=false
    rerender(
      <LabelGrid
        rows={[row]}
        fields={["operable", "missingEntry", "listCol", "imageCol"]}
        cellInput={cellInput}
        disabled={false}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(operableCell).toHaveAttribute("aria-readonly", "false");
  });
});

describe("4.5 Regression checks on existing grid behavior", () => {
  it("renders columns in the exact order specified by the fields array", () => {
    const fields = ["sku", "orientation", "tags", "notes"];
    render(
      <LabelGrid
        rows={sampleRows()}
        fields={fields}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const headers = screen.getAllByRole("columnheader").map((h) => h.textContent?.trim());
    // Excluding __annotation ("Status") and __actions ("")
    expect(headers).toContain("sku");
    expect(headers).toContain("orientation");
    expect(headers).toContain("tags");
    expect(headers).toContain("notes");

    const skuIdx = headers.indexOf("sku");
    const orientIdx = headers.indexOf("orientation");
    const tagsIdx = headers.indexOf("tags");
    const notesIdx = headers.indexOf("notes");

    expect(skuIdx).toBeLessThan(orientIdx);
    expect(orientIdx).toBeLessThan(tagsIdx);
    expect(tagsIdx).toBeLessThan(notesIdx);
  });

  it("preserves per-row editability: one row editable, another row inert with '—'", () => {
    const mixedRows: LabelGridRow[] = [
      { id: "r1", origin: "csv", data: { notes: "active note" }, validation: {} },
      { id: "r2", origin: "csv", data: { notes: "inactive note" }, validation: {} },
    ];
    const cellInput = (r: LabelGridRow, f: string): InputSpec | undefined => {
      if (r.id === "r1" && f === "notes") return { name: f, control: "text" };
      return undefined; // inactive on r2
    };

    render(
      <LabelGrid
        rows={mixedRows}
        fields={["notes"]}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    // Row 1 has operable input
    expect(screen.getByLabelText("edit notes")).toBeInTheDocument();
    expect(screen.getByDisplayValue("active note")).toBeInTheDocument();

    // Row 2 renders plain em-dash
    expect(screen.getByText("—")).toBeInTheDocument();
  });

  it("select retains out-of-range value in options and commits held value without change", async () => {
    const spec: InputSpec = {
      name: "size",
      control: "select",
      values: ["small", "medium", "large"],
    };
    let currentRows: LabelGridRow[] = [
      { id: "r1", origin: "csv", data: { size: "enormous" }, validation: {} },
    ];
    const onRowsChange = vi.fn((next: LabelGridRow[]) => {
      currentRows = next;
    });

    render(
      <LabelGrid
        rows={currentRows}
        fields={["size"]}
        cellInput={(_r, f) => (f === "size" ? spec : undefined)}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const select = screen.getByLabelText("edit size") as HTMLSelectElement;
    expect(select.value).toBe("enormous");
    const options = Array.from(select.querySelectorAll("option")).map((o) => o.value);
    expect(options).toEqual(["", "small", "medium", "large", "enormous"]);

    // Leaving without change keeps held value in row data and pruneDataForSubmit
    expect(onRowsChange).not.toHaveBeenCalled();
    expect(currentRows[0].data.size).toBe("enormous");
    expect(pruneDataForSubmit(currentRows[0].data, [spec])).toEqual({ size: "enormous" });

    // Changing to declared value "medium" commits "medium"
    fireEvent.change(select, { target: { value: "medium" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));
    expect(currentRows[0].data.size).toBe("medium");
  });

  it("unset select shows (none) and submits no key via pruneDataForSubmit", () => {
    const spec: InputSpec = {
      name: "size",
      control: "select",
      values: ["small", "medium", "large"],
    };
    const row: LabelGridRow = {
      id: "r1",
      origin: "csv",
      data: { size: "" },
      validation: {},
    };

    render(
      <LabelGrid
        rows={[row]}
        fields={["size"]}
        cellInput={(_r, f) => (f === "size" ? spec : undefined)}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const select = screen.getByLabelText("edit size") as HTMLSelectElement;
    expect(select.value).toBe("");
    expect(select.querySelector('option[value=""]')?.textContent).toBe("(none)");

    const pruned = pruneDataForSubmit(row.data, [spec]);
    expect(pruned).not.toHaveProperty("size");
  });

  it("commits through onRowsChange with updated rows and accurate change indexes", async () => {
    let currentRows = sampleRows();
    const onRowsChange = vi.fn<LabelGridProps["onRowsChange"]>((next) => {
      currentRows = next;
    });

    render(
      <LabelGrid
        rows={currentRows}
        fields={["sku", "notes"]}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    const skuInputs = screen.getAllByLabelText("edit sku");
    expect(skuInputs).toHaveLength(2);

    // Edit row 2 (index 1)
    fireEvent.change(skuInputs[1], { target: { value: "999" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));

    const [updatedRows, change] = onRowsChange.mock.calls[0];
    expect(updatedRows[1].data.sku).toBe("999");
    expect(updatedRows[0].data.sku).toBe("1"); // Row 0 untouched
    expect(change).toEqual({ indexes: [1] });
  });
});
