import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { LabelGrid } from "./LabelGrid";
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

describe("LabelGrid selection", () => {
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
});

function rows(): LabelGridRow[] {
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

const props = {
  fields: ["sku", "notes"],
};

describe("LabelGrid", () => {
  it("renders data cell values", () => {
    render(<LabelGrid rows={rows()} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("first")).toBeInTheDocument();
  });

  it("shows the annotation message for a failed row", () => {
    render(<LabelGrid rows={rows()} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByText(/boom/)).toBeInTheDocument();
  });

  it("shows validation errors: an empty required field", () => {
    const rs: LabelGridRow[] = [
      { id: "a", origin: "csv", data: { sku: "", notes: "ok" }, validation: { field: { sku: "required" } } },
    ];
    render(<LabelGrid rows={rs} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByLabelText(/sku required/i)).toBeInTheDocument();
  });

  it("calls onDuplicate and onRemove with the row id", () => {
    const onDuplicate = vi.fn();
    const onRemove = vi.fn();
    render(<LabelGrid rows={rows()} {...props} onRowsChange={() => {}} onDuplicate={onDuplicate} onRemove={onRemove} />);
    fireEvent.click(screen.getAllByRole("button", { name: /duplicate/i })[0]);
    fireEvent.click(screen.getAllByRole("button", { name: /remove/i })[0]);
    expect(onDuplicate).toHaveBeenCalledWith("a");
    expect(onRemove).toHaveBeenCalledWith("a");
  });

  it("opens no editor and disables the row actions while disabled", async () => {
    const onRowsChange = vi.fn();
    render(
      <LabelGrid rows={rows()} {...props} disabled onRowsChange={onRowsChange} onDuplicate={() => {}} onRemove={() => {}} />,
    );
    fireEvent.doubleClick(screen.getByText("1"));
    expect(screen.queryByLabelText("edit sku")).toBeNull();
    expect(screen.getAllByRole("button", { name: /duplicate/i })[0]).toBeDisabled();
    expect(screen.getAllByRole("button", { name: /remove/i })[0]).toBeDisabled();
    expect(onRowsChange).not.toHaveBeenCalled();
  });

  it("commits a nested data-cell edit through onRowsChange", async () => {
    const onRowsChange = vi.fn();
    render(<LabelGrid rows={rows()} {...props} onRowsChange={onRowsChange} onDuplicate={() => {}} onRemove={() => {}} />);
    fireEvent.doubleClick(screen.getByText("1"));
    const input = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    expect(input.tagName).toBe("INPUT");
    fireEvent.change(input, { target: { value: "9" } });
    fireEvent.blur(input);
    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[0].data.sku).toBe("9");
    // Which rows the edit touched is this component's own bookkeeping, and both callers clear the
    // edited row's stale annotation from it.
    expect(onRowsChange.mock.calls.at(-1)![1]).toEqual({ indexes: [0] });
  });

  it("renders an input element for a text-control cell and commits on blur", async () => {
    const onRowsChange = vi.fn();
    const cellInput = (_row: LabelGridRow, field: string) => ({ name: field, control: "text" as const });
    render(
      <LabelGrid
        rows={rows()}
        {...props}
        cellInput={cellInput}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );
    fireEvent.doubleClick(screen.getByText("1"));
    const input = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    expect(input.tagName).toBe("INPUT");
    fireEvent.change(input, { target: { value: "9" } });
    fireEvent.blur(input);
    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[0].data.sku).toBe("9");
  });

  it("commits on Tab and moves to the next editable cell, skipping an inert one", async () => {
    const onRowsChange = vi.fn();
    const tabRows: LabelGridRow[] = [
      { id: "a", origin: "csv", data: { sku: "1", locked: "x", notes: "first" }, validation: {} },
    ];
    const cellInput = (_row: LabelGridRow, field: string) =>
      field === "locked" ? undefined : { name: field, control: "text" as const };

    render(
      <LabelGrid
        rows={tabRows}
        fields={["sku", "locked", "notes"]}
        cellInput={cellInput}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    fireEvent.doubleClick(screen.getByText("1"));
    const input = await screen.findByLabelText("edit sku");
    fireEvent.change(input, { target: { value: "9" } });
    // `code` matters: the grid's hotkeys read KeyboardEvent.code, not .key.
    fireEvent.keyDown(input, { key: "Tab", code: "Tab" });

    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    expect((onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[])[0].data.sku).toBe("9");
    expect(await screen.findByLabelText("edit notes")).toBeInTheDocument();
    expect(screen.queryByLabelText("edit locked")).toBeNull();
  });

  it("renders inert cell with '—' and disables editing when cellInput returns undefined", async () => {
    const cellInput = (row: LabelGridRow, field: string) => {
      // notes is inactive on row 'a'
      if (row.id === "a" && field === "notes") return undefined;
      return { name: field, control: "text" as const };
    };

    const { rerender } = render(
      <LabelGrid
        rows={rows()}
        {...props}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    // Row 'a' notes cell should render inert '—'
    expect(screen.getByText("—")).toBeInTheDocument();
    // Row 'b' notes cell should render 'second'
    expect(screen.getByText("second")).toBeInTheDocument();

    // Trying to double-click the inert cell should not open an edit input
    fireEvent.doubleClick(screen.getByText("—"));
    expect(screen.queryByLabelText("edit notes")).toBeNull();

    // When cellInput becomes defined again, the stored value returns
    rerender(
      <LabelGrid
        rows={rows()}
        {...props}
        cellInput={(_row, field) => ({ name: field, control: "text" as const })}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(screen.getByText("first")).toBeInTheDocument();
  });

  it("commits a text cell on Enter, against the grid reading a bubbled Enter as a cancel", async () => {
    const onRowsChange = vi.fn();
    render(<LabelGrid rows={rows()} {...props} onRowsChange={onRowsChange} onDuplicate={() => {}} onRemove={() => {}} />);
    // The second row, so a hard-coded first index cannot pass for the one the edit touched.
    fireEvent.doubleClick(screen.getByText("2"));
    const input = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "9" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[1].data.sku).toBe("9");
    expect(onRowsChange.mock.calls.at(-1)![1]).toEqual({ indexes: [1] });
    expect(screen.queryByLabelText("edit sku")).toBeNull();
  });

  it("leaves prior value intact when Escape is pressed in a text cell", async () => {
    const onRowsChange = vi.fn();
    render(<LabelGrid rows={rows()} {...props} onRowsChange={onRowsChange} onDuplicate={() => {}} onRemove={() => {}} />);
    fireEvent.doubleClick(screen.getByText("1"));
    const input = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "typed change" } });

    fireEvent.keyDown(input, { key: "Escape" });

    expect(screen.queryByLabelText("edit sku")).toBeNull();
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(onRowsChange).not.toHaveBeenCalled();
  });

  it("drops an edit whose row left the grid while its editor was open", async () => {
    const onRowsChange = vi.fn();
    const both = rows();
    const gridProps = { ...props, onRowsChange, onDuplicate: () => {}, onRemove: () => {} };
    const { rerender } = render(<LabelGrid rows={both} {...gridProps} />);

    fireEvent.doubleClick(screen.getByText("1"));
    const input = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "9" } });

    // Row 'a' is removed (a print run resetting the rows, say) while its editor still holds an edit.
    rerender(<LabelGrid rows={both.slice(1)} {...gridProps} />);
    // Opening another cell's editor is what commits the open one, and it has nowhere to land.
    fireEvent.doubleClick(screen.getByText("2"));
    await screen.findByLabelText("edit sku");

    expect(onRowsChange).not.toHaveBeenCalled();
  });

  it("edits a textarea cell with Shift+Enter inserting a newline and commits on blur", async () => {
    const onRowsChange = vi.fn();
    const cellInput = (_row: LabelGridRow, field: string) =>
      field === "notes" ? ({ name: "notes", control: "textarea" } as const) : ({ name: field, control: "text" } as const);

    render(
      <LabelGrid
        rows={rows()}
        {...props}
        cellInput={cellInput}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    fireEvent.doubleClick(screen.getByText("first"));
    const textarea = (await screen.findByLabelText("edit notes")) as HTMLTextAreaElement;
    expect(textarea.tagName).toBe("TEXTAREA");

    expect(fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true })).toBe(true);
    fireEvent.change(textarea, { target: { value: "first\nline" } });
    fireEvent.blur(textarea);

    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[0].data.notes).toBe("first\nline");
  });

  it("commits a textarea edit on plain Enter without inserting a newline", async () => {
    const onRowsChange = vi.fn();
    const cellInput = (_row: LabelGridRow, field: string) =>
      field === "notes" ? ({ name: "notes", control: "textarea" } as const) : ({ name: field, control: "text" } as const);

    render(
      <LabelGrid
        rows={rows()}
        {...props}
        cellInput={cellInput}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    fireEvent.doubleClick(screen.getByText("first"));
    const textarea = (await screen.findByLabelText("edit notes")) as HTMLTextAreaElement;
    expect(textarea.tagName).toBe("TEXTAREA");
    fireEvent.change(textarea, { target: { value: "first edit" } });

    expect(fireEvent.keyDown(textarea, { key: "Enter", shiftKey: false })).toBe(false);

    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[0].data.notes).toBe("first edit");
  });

  it("leaves prior value intact when Escape is pressed in textarea edit", async () => {
    const onRowsChange = vi.fn();
    const cellInput = (_row: LabelGridRow, field: string) =>
      field === "notes" ? ({ name: "notes", control: "textarea" } as const) : ({ name: field, control: "text" } as const);

    render(
      <LabelGrid
        rows={rows()}
        {...props}
        cellInput={cellInput}
        onRowsChange={onRowsChange}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    fireEvent.doubleClick(screen.getByText("first"));
    const textarea = (await screen.findByLabelText("edit notes")) as HTMLTextAreaElement;
    expect(textarea.tagName).toBe("TEXTAREA");
    fireEvent.change(textarea, { target: { value: "typed change" } });

    fireEvent.keyDown(textarea, { key: "Escape" });

    expect(screen.queryByLabelText("edit notes")).toBeNull();
    expect(screen.getByText("first")).toBeInTheDocument();
    expect(onRowsChange).not.toHaveBeenCalled();
  });

  it("renders a multiline cell differently from a single-line cell with a line count marker, splitting on CRLF and LF alike", () => {
    const multilineRows: LabelGridRow[] = [
      { id: "a", origin: "csv", data: { sku: "1", notes: "line one\nline two" }, validation: {} },
      { id: "b", origin: "csv", data: { sku: "2", notes: "line one line two" }, validation: {} },
      { id: "c", origin: "csv", data: { sku: "3", notes: "crlf one\r\ncrlf two" }, validation: {} },
    ];

    render(
      <LabelGrid
        rows={multilineRows}
        {...props}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(screen.getByText("line one")).toBeInTheDocument();
    expect(screen.getAllByText("+1")).toHaveLength(2);
    expect(screen.getByText("line one line two")).toBeInTheDocument();
    expect(screen.getByText("crlf one")).toBeInTheDocument();
  });

  it("exposes validation error message on a multiline cell in title and style", () => {
    const multilineInvalidRows: LabelGridRow[] = [
      {
        id: "a",
        origin: "csv",
        data: { sku: "1", notes: "line one\nline two" },
        validation: { field: { notes: "invalid format" } },
      },
    ];

    render(
      <LabelGrid
        rows={multilineInvalidRows}
        {...props}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(screen.getByText("line one")).toBeInTheDocument();
    expect(screen.getByText("+1")).toBeInTheDocument();

    const cellSpan = screen.getByText("line one").parentElement;
    expect(cellSpan).toHaveAttribute("title", "invalid format\n\nline one\nline two");
    expect(cellSpan).toHaveStyle({ color: "var(--bad)" });
  });

  it("renders inert cell with '—' and disables editing when cellInput control is 'list' and cell is not an array", () => {
    const cellInput = (_row: LabelGridRow, field: string) => {
      if (field === "notes") return { name: "notes", control: "list" as const };
      return { name: field, control: "text" as const };
    };

    render(
      <LabelGrid
        rows={rows()}
        {...props}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(screen.getAllByText("—")).toHaveLength(2);
    expect(screen.getByText("1")).toBeInTheDocument();

    fireEvent.doubleClick(screen.getAllByText("—")[0]);
    expect(screen.queryByLabelText("edit notes")).toBeNull();
  });

  it("renders a list-control column holding an array as displayCellText and is not editable", () => {
    const listRows: LabelGridRow[] = [
      {
        id: "r1",
        origin: "connector",
        data: { sku: "100", tags: ["KIDS", "CONSUMABLE"] },
        validation: {},
      },
      {
        id: "r2",
        origin: "connector",
        data: { sku: "101", tags: [] },
        validation: {},
      },
      {
        id: "r3",
        origin: "connector",
        data: { sku: "102" },
        validation: {},
      },
    ];

    const cellInput = (_row: LabelGridRow, field: string) => {
      if (field === "tags") return { name: "tags", control: "list" as const };
      return { name: field, control: "text" as const };
    };

    render(
      <LabelGrid
        rows={listRows}
        fields={["sku", "tags"]}
        cellInput={cellInput}
        onRowsChange={() => {}}
        onDuplicate={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(screen.getByText("KIDS, CONSUMABLE")).toBeInTheDocument();
    // Non-array row r3 displays em-dash
    expect(screen.getByText("—")).toBeInTheDocument();

    // Double clicking list cell does not open edit input
    fireEvent.doubleClick(screen.getByText("KIDS, CONSUMABLE"));
    expect(screen.queryByLabelText("edit tags")).toBeNull();
  });

  describe("Issue #271 - control-aware grid editors and resting cells", () => {
    it.each([
      { control: "select" as const, expectedTag: "SELECT" },
      { control: "checkbox" as const, expectedTag: "INPUT", expectedType: "checkbox" },
      { control: "integer" as const, expectedTag: "INPUT", expectedType: "number" },
      { control: "number" as const, expectedTag: "INPUT", expectedType: "number" },
      { control: "date" as const, expectedTag: "INPUT", expectedType: "date" },
      { control: "datetime" as const, expectedTag: "INPUT", expectedType: "datetime-local" },
    ])("1.1 opens $control editor instead of text editor fallback", async ({ control, expectedTag, expectedType }) => {
      const row: LabelGridRow = { id: "r1", origin: "csv", data: { field1: "val" }, validation: {} };
      const cellInput = (_r: LabelGridRow, f: string): InputSpec => ({
        name: f,
        control,
        values: control === "select" ? ["val", "other"] : undefined,
      });
      const { container } = render(
        <LabelGrid
          rows={[row]}
          fields={["field1"]}
          cellInput={cellInput}
          onRowsChange={() => {}}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );
      const cell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:field1"]')!;
      fireEvent.doubleClick(cell);
      const editor = (await screen.findByLabelText("edit field1")) as HTMLElement;
      expect(editor.tagName).toBe(expectedTag);
      if (expectedType) {
        expect(editor).toHaveAttribute("type", expectedType);
      }
    });

    it("1.2 select: offers declared values plus empty choice, retains undeclared held value, commits held value without change, and abandons on Escape", async () => {
      const r1Spec: InputSpec = {
        name: "size",
        control: "select",
        values: ["small", "medium", "large"],
      };
      const r2Spec: InputSpec = {
        name: "size",
        control: "select",
        values: ["compact", "standard", "oversized"],
      };
      const cellInput = (r: LabelGridRow, f: string): InputSpec | undefined => {
        if (f !== "size") return undefined;
        return r.id === "r1" ? r1Spec : r2Spec;
      };

      let currentRows: LabelGridRow[] = [
        { id: "r1", origin: "csv", data: { size: "" }, validation: {} },
        { id: "r2", origin: "csv", data: { size: "enormous" }, validation: {} },
      ];
      const onRowsChange = vi.fn((newRows: LabelGridRow[]) => {
        currentRows = newRows;
      });

      const { container } = render(
        <LabelGrid
          rows={currentRows}
          fields={["size"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // 0. Resting cells: unset r1 shows that nothing is chosen ("(none)"); r2 shows "enormous"
      expect(screen.getByText("(none)")).toBeInTheDocument();
      expect(screen.getByText("enormous")).toBeInTheDocument();

      // 1. Unset cell's editor offers exactly the three declared values plus the choice standing for nothing chosen
      const r1Cell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:size"]')!;
      fireEvent.doubleClick(r1Cell);
      const editor1 = (await screen.findByLabelText("edit size")) as HTMLSelectElement;
      expect(editor1.tagName).toBe("SELECT");
      const r1Options = Array.from(editor1.querySelectorAll("option")).map((o) => o.value);
      expect(r1Options).toEqual(["", "small", "medium", "large"]);
      expect(editor1.querySelector('option[value=""]')?.textContent).toBe("(none)");

      // Abandon editor1
      fireEvent.keyDown(editor1, { key: "Escape" });

      // 2. A cell holding 'enormous' on r2 resolves r2's entry and also offers 'enormous' and nothing else
      const r2Cell = container.querySelector('[data-row-id$="r2"][data-col-id$="data:size"]')!;
      fireEvent.doubleClick(r2Cell);
      const editor2 = (await screen.findByLabelText("edit size")) as HTMLSelectElement;
      expect(editor2.tagName).toBe("SELECT");
      const r2Options = Array.from(editor2.querySelectorAll("option")).map((o) => o.value);
      expect(r2Options).toEqual(["", "compact", "standard", "oversized", "enormous"]);
      expect(editor2.querySelector('option[value=""]')?.textContent).toBe("(none)");

      // Picking another value in the open editor retains the held value in options so it can be re-selected
      fireEvent.change(editor2, { target: { value: "" } });
      const r2OptionsAfterPick = Array.from(editor2.querySelectorAll("option")).map((o) => o.value);
      expect(r2OptionsAfterPick).toContain("enormous");
      fireEvent.change(editor2, { target: { value: "enormous" } });

      // 3. Opening and committing without choosing leaves 'enormous' in the cell and in the submitted row
      fireEvent.keyDown(editor2, { key: "Enter" });
      await waitFor(() => expect(screen.queryByLabelText("edit size")).toBeNull());
      expect(screen.getByText("enormous")).toBeInTheDocument();
      // Committing without choosing fires no onRowsChange, proving caller row data was left unaltered:
      expect(onRowsChange).not.toHaveBeenCalled();
      const r2AfterCommit = currentRows.find((r) => r.id === "r2")!;
      expect(r2AfterCommit.data.size).toBe("enormous");
      const pruned = pruneDataForSubmit(r2AfterCommit.data, [r2Spec]);
      expect(pruned.size).toBe("enormous");

      // 4. Escape abandons a chosen option and stops propagation so the grid does not act on the key
      const r2CellReopened = container.querySelector('[data-row-id$="r2"][data-col-id$="data:size"]')!;
      fireEvent.doubleClick(r2CellReopened);
      const editor3 = (await screen.findByLabelText("edit size")) as HTMLSelectElement;
      fireEvent.change(editor3, { target: { value: "standard" } });
      const escapeEvent = new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true });
      const stopPropagationSpy = vi.spyOn(escapeEvent, "stopPropagation");
      editor3.dispatchEvent(escapeEvent);
      expect(stopPropagationSpy).toHaveBeenCalled();
      await waitFor(() => expect(screen.queryByLabelText("edit size")).toBeNull());
      expect(screen.getByText("enormous")).toBeInTheDocument();
      expect(currentRows.find((r) => r.id === "r2")!.data.size).toBe("enormous");
    });

    it("1.3 checkbox: one activation from unset yields true and ticked resting box; three activations cycle unset -> checked -> unchecked -> unset and submits no key", async () => {
      let currentRows: LabelGridRow[] = [
        { id: "r1", origin: "csv", data: { active: "" }, validation: {} },
      ];
      const spec: InputSpec = { name: "active", control: "checkbox" };
      const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => (f === "active" ? spec : undefined);
      const onRowsChange = vi.fn((newRows: LabelGridRow[]) => {
        currentRows = newRows;
      });

      const { container, rerender } = render(
        <LabelGrid
          rows={currentRows}
          fields={["active"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // Activation 1 from unset:
      const cell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:active"]')!;
      fireEvent.doubleClick(cell);
      const editor1 = (await screen.findByLabelText("edit active")) as HTMLInputElement;
      expect(editor1.tagName).toBe("INPUT");
      expect(editor1).toHaveAttribute("type", "checkbox");
      // The open editor on an unset cell draws with indeterminate (task 3.2):
      expect(editor1.indeterminate).toBe(true);
      expect(editor1.checked).toBe(false);
      // Activate once
      fireEvent.click(editor1);
      fireEvent.blur(editor1);
      await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));
      expect(currentRows[0].data.active).toBe("true");

      rerender(
        <LabelGrid
          rows={currentRows}
          fields={["active"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // Resting cell has a ticked resting box rather than the text "true"
      expect(screen.queryByText("true")).toBeNull();
      const restingBox1 = screen.getByRole("checkbox", { name: /active checked/i }) as HTMLInputElement;
      expect(restingBox1).toBeChecked();
      expect(restingBox1).toHaveAttribute("aria-disabled", "true");

      // Activation 2: checked -> unchecked
      const cellChecked = container.querySelector('[data-row-id$="r1"][data-col-id$="data:active"]')!;
      fireEvent.doubleClick(cellChecked);
      const editor2 = (await screen.findByLabelText("edit active")) as HTMLInputElement;
      fireEvent.click(editor2);
      fireEvent.blur(editor2);
      await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(2));
      expect(currentRows[0].data.active).toBe("false");

      rerender(
        <LabelGrid
          rows={currentRows}
          fields={["active"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );
      expect(screen.queryByText("false")).toBeNull();
      const restingBox2 = screen.getByRole("checkbox", { name: /active unchecked/i }) as HTMLInputElement;
      expect(restingBox2).not.toBeChecked();
      expect(restingBox2).toHaveAttribute("aria-disabled", "true");

      // Activation 3: unchecked -> unset
      const cellUnchecked = container.querySelector('[data-row-id$="r1"][data-col-id$="data:active"]')!;
      fireEvent.doubleClick(cellUnchecked);
      const editor3 = (await screen.findByLabelText("edit active")) as HTMLInputElement;
      fireEvent.click(editor3);
      fireEvent.blur(editor3);
      await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(3));
      expect(currentRows[0].data.active).toBe("");

      rerender(
        <LabelGrid
          rows={currentRows}
          fields={["active"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );
      const restingBox3 = screen.getByRole("checkbox", { name: /active unset/i }) as HTMLInputElement;
      expect(restingBox3).not.toBeChecked();
      expect(restingBox3.indeterminate).toBe(true);
      expect(restingBox3).toHaveAttribute("aria-disabled", "true");

      // Row submits no key for that name once unset
      const pruned = pruneDataForSubmit(currentRows[0].data, [spec]);
      expect(pruned).not.toHaveProperty("active");
    });

    it("1.4 integer and number: editor carries min/max, reports invalid and constrains stepping; integer steps by 1, number accepts 2.5; slider: true opens plain numeric control", async () => {
      const numericRows: LabelGridRow[] = [
        { id: "r1", origin: "csv", data: { count: "5", ratio: "1.0", sliderInt: "3" }, validation: {} },
      ];
      const specs: Record<string, InputSpec> = {
        count: { name: "count", control: "integer", min: 1, max: 9 },
        ratio: { name: "ratio", control: "number" },
        sliderInt: { name: "sliderInt", control: "integer", min: 1, max: 10, slider: true },
      };
      const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => specs[f];

      const { container } = render(
        <LabelGrid
          rows={numericRows}
          fields={["count", "ratio", "sliderInt"]}
          cellInput={cellInput}
          onRowsChange={() => {}}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // count (integer with min: 1, max: 9):
      const countCell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:count"]')!;
      fireEvent.doubleClick(countCell);
      const countEditor = (await screen.findByLabelText("edit count")) as HTMLInputElement;
      expect(countEditor.tagName).toBe("INPUT");
      expect(countEditor).toHaveAttribute("type", "number");
      expect(countEditor).toHaveAttribute("min", "1");
      expect(countEditor).toHaveAttribute("max", "9");
      expect(countEditor).toHaveAttribute("step", "1");

      // Cannot be stepped past 9
      countEditor.value = "9";
      countEditor.stepUp();
      expect(countEditor.value).toBe("9");

      // Reports typed 20 invalid under max: 9
      fireEvent.change(countEditor, { target: { value: "20" } });
      expect(countEditor).toHaveAttribute("aria-invalid", "true");

      // Abandon count editor
      fireEvent.keyDown(countEditor, { key: "Escape" });

      // ratio (number with no bounds):
      const ratioCell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:ratio"]')!;
      fireEvent.doubleClick(ratioCell);
      const ratioEditor = (await screen.findByLabelText("edit ratio")) as HTMLInputElement;
      expect(ratioEditor).toHaveAttribute("type", "number");
      expect(ratioEditor).toHaveAttribute("step", "any");
      fireEvent.change(ratioEditor, { target: { value: "2.5" } });
      expect(ratioEditor).toHaveAttribute("aria-invalid", "false");
      fireEvent.keyDown(ratioEditor, { key: "Escape" });

      // sliderInt (slider: true): opens plain numeric control (type="number"), not type="range"
      const sliderCell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:sliderInt"]')!;
      fireEvent.doubleClick(sliderCell);
      const sliderEditor = (await screen.findByLabelText("edit sliderInt")) as HTMLInputElement;
      expect(sliderEditor).toHaveAttribute("type", "number");
    });

    it("1.5 date and datetime: editors are date and datetime-local controls; date holding RFC 3339 still holds it after open and commit without typing", async () => {
      let currentRows: LabelGridRow[] = [
        { id: "r1", origin: "csv", data: { d: "2026-09-01T12:00:00Z", dt: "2026-09-01T12:00" }, validation: {} },
      ];
      const specs: Record<string, InputSpec> = {
        d: { name: "d", control: "date" },
        dt: { name: "dt", control: "datetime" },
      };
      const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => specs[f];
      const onRowsChange = vi.fn((newRows: LabelGridRow[]) => {
        currentRows = newRows;
      });

      const { container, rerender } = render(
        <LabelGrid
          rows={currentRows}
          fields={["d", "dt"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // d editor: date control
      const dCell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:d"]')!;
      fireEvent.doubleClick(dCell);
      const dEditor = (await screen.findByLabelText("edit d")) as HTMLInputElement;
      expect(dEditor.tagName).toBe("INPUT");
      expect(dEditor).toHaveAttribute("type", "date");

      // Commit without typing
      fireEvent.keyDown(dEditor, { key: "Enter" });
      await waitFor(() => expect(screen.queryByLabelText("edit d")).toBeNull());

      // The cell still holds 2026-09-01T12:00:00Z
      rerender(
        <LabelGrid
          rows={currentRows}
          fields={["d", "dt"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );
      expect(screen.getByText("2026-09-01T12:00:00Z")).toBeInTheDocument();
      expect(currentRows[0].data.d).toBe("2026-09-01T12:00:00Z");

      // dt editor: date-and-time control
      const dtCell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:dt"]')!;
      fireEvent.doubleClick(dtCell);
      const dtEditor = (await screen.findByLabelText("edit dt")) as HTMLInputElement;
      expect(dtEditor).toHaveAttribute("type", "datetime-local");
    });

    it("1.6 image: double-clicking opens no editor, cell shows image marker instead of data URI, and row submits held value", async () => {
      const dataUri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
      let currentRows: LabelGridRow[] = [
        { id: "r1", origin: "csv", data: { logo: dataUri }, validation: {} },
      ];
      const onRowsChange = vi.fn((newRows: LabelGridRow[]) => {
        currentRows = newRows;
      });
      const spec: InputSpec = { name: "logo", control: "image" };
      const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => (f === "logo" ? spec : undefined);

      const { container } = render(
        <LabelGrid
          rows={currentRows}
          fields={["logo"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // Shows that it holds an image rather than its data URI
      expect(screen.queryByText(dataUri)).toBeNull();
      expect(screen.getByText("image")).toBeInTheDocument();

      // Double-clicking opens no editor
      const logoCell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:logo"]')!;
      fireEvent.doubleClick(logoCell);
      expect(screen.queryByLabelText("edit logo")).toBeNull();
      // Opening image cell does not fire onRowsChange (0 calls), proving the grid left caller row data untouched:
      expect(onRowsChange).not.toHaveBeenCalled();

      // Row still submits held value (combination proof: untouched caller row data preserves held value)
      expect(currentRows[0].data.logo).toBe(dataUri);
      const pruned = pruneDataForSubmit(currentRows[0].data, [spec]);
      expect(pruned.logo).toBe(dataUri);
    });

    it("1.7 unset checkbox reads as neither checked nor unchecked, unset select does not read as first declared value, neither submitted; empty flagged cell keeps error marker", async () => {
      const specCheckbox: InputSpec = { name: "active", control: "checkbox" };
      const specSelect: InputSpec = { name: "size", control: "select", values: ["small", "medium", "large"] };
      const specs: Record<string, InputSpec> = { active: specCheckbox, size: specSelect };
      const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => specs[f];

      let currentRows: LabelGridRow[] = [
        { id: "r1", origin: "csv", data: { active: "", size: "" }, validation: {} },
      ];
      const onRowsChange = vi.fn((newRows: LabelGridRow[]) => {
        currentRows = newRows;
      });

      const { container, rerender } = render(
        <LabelGrid
          rows={currentRows}
          fields={["active", "size"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // Checkbox cell reads as neither checked nor unchecked (indeterminate)
      const box = screen.getByRole("checkbox", { name: "active unset" }) as HTMLInputElement;
      expect(box).not.toBeChecked();
      expect(box.indeterminate).toBe(true);
      expect(box).toHaveAttribute("aria-disabled", "true");

      // Unset select shows that nothing is chosen ("(none)") and does not read as its first declared value ("small")
      expect(screen.getByText("(none)")).toBeInTheDocument();
      expect(screen.queryByText("small")).toBeNull();

      // Opening and committing without choosing leaves both cells unset ("") rather than defaulting
      // to false or the first declared value ("small").
      const activeCell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:active"]')!;
      fireEvent.doubleClick(activeCell);
      const activeEditor = (await screen.findByLabelText("edit active")) as HTMLInputElement;
      fireEvent.keyDown(activeEditor, { key: "Enter" });
      await waitFor(() => expect(screen.queryByLabelText("edit active")).toBeNull());

      const sizeCell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:size"]')!;
      fireEvent.doubleClick(sizeCell);
      const sizeEditor = (await screen.findByLabelText("edit size")) as HTMLSelectElement;
      fireEvent.keyDown(sizeEditor, { key: "Enter" });
      await waitFor(() => expect(screen.queryByLabelText("edit size")).toBeNull());

      // Committing without choosing fires no onRowsChange (0 calls), proving the grid did not alter
      // caller row data to default values (e.g. false or first declared option).
      expect(onRowsChange).not.toHaveBeenCalled();

      // In combination with untouched caller row data, pruneDataForSubmit submits neither key
      expect(currentRows[0].data.active).toBe("");
      expect(currentRows[0].data.size).toBe("");
      const pruned = pruneDataForSubmit(currentRows[0].data, [specCheckbox, specSelect]);
      expect(pruned).not.toHaveProperty("active");
      expect(pruned).not.toHaveProperty("size");

      // An empty cell its row flags keeps the ⚠ <error> marker whatever its control
      const rowsWithError: LabelGridRow[] = [
        {
          id: "r1",
          origin: "csv",
          data: { active: "", size: "" },
          validation: { field: { active: "required", size: "required" } },
        },
      ];

      rerender(
        <LabelGrid
          rows={rowsWithError}
          fields={["active", "size"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      expect(screen.getByLabelText("active required")).toHaveTextContent("⚠ required");
      expect(screen.getByLabelText("size required")).toHaveTextContent("⚠ required");
    });
  });

  describe("Issue #271 - review regression guards", () => {
    it("checkbox with unrecognized value (e.g. 'maybe') renders as text, opens checkbox editor without indeterminate, and commits untouched without activation", async () => {
      let currentRows: LabelGridRow[] = [
        { id: "r1", origin: "csv", data: { flag: "maybe" }, validation: {} },
      ];
      const spec: InputSpec = { name: "flag", control: "checkbox" };
      const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => (f === "flag" ? spec : undefined);
      const onRowsChange = vi.fn((newRows: LabelGridRow[]) => {
        currentRows = newRows;
      });

      const { container } = render(
        <LabelGrid
          rows={currentRows}
          fields={["flag"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // At rest, unrecognized value renders as text rather than a checkbox state
      expect(screen.getByText("maybe")).toBeInTheDocument();

      // Open editor: unrecognized value opens a checkbox editor that does not report as unset (indeterminate is false)
      const cell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:flag"]')!;
      fireEvent.doubleClick(cell);
      const editor = (await screen.findByLabelText("edit flag")) as HTMLInputElement;
      expect(editor.tagName).toBe("INPUT");
      expect(editor).toHaveAttribute("type", "checkbox");
      expect(editor.indeterminate).toBe(false);
      expect(editor.checked).toBe(false);

      // Commit without activation: retains 'maybe' unaltered
      fireEvent.keyDown(editor, { key: "Enter" });
      await waitFor(() => expect(screen.queryByLabelText("edit flag")).toBeNull());
      expect(screen.getByText("maybe")).toBeInTheDocument();
      expect(currentRows[0].data.flag).toBe("maybe");
      expect(onRowsChange).not.toHaveBeenCalled();
    });

    it("checkbox editor first activation from unrecognized value enters cycle at checked ('true'), then cycles to unchecked and unset", async () => {
      let currentRows: LabelGridRow[] = [
        { id: "r1", origin: "csv", data: { flag: "maybe" }, validation: {} },
      ];
      const spec: InputSpec = { name: "flag", control: "checkbox" };
      const cellInput = (_r: LabelGridRow, f: string): InputSpec | undefined => (f === "flag" ? spec : undefined);
      const onRowsChange = vi.fn((newRows: LabelGridRow[]) => {
        currentRows = newRows;
      });

      const { container, rerender } = render(
        <LabelGrid
          rows={currentRows}
          fields={["flag"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );

      // 1. Activation from unrecognized value transitions to checked ("true")
      const cell = container.querySelector('[data-row-id$="r1"][data-col-id$="data:flag"]')!;
      fireEvent.doubleClick(cell);
      const editor1 = (await screen.findByLabelText("edit flag")) as HTMLInputElement;
      expect(editor1.tagName).toBe("INPUT");
      expect(editor1).toHaveAttribute("type", "checkbox");
      expect(editor1.indeterminate).toBe(false);
      expect(editor1.checked).toBe(false);

      fireEvent.click(editor1);
      fireEvent.blur(editor1);
      await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(1));
      expect(currentRows[0].data.flag).toBe("true");

      rerender(
        <LabelGrid
          rows={currentRows}
          fields={["flag"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );
      const restingBox1 = screen.getByRole("checkbox", { name: /flag checked/i }) as HTMLInputElement;
      expect(restingBox1).toBeChecked();
      expect(restingBox1).toHaveAttribute("aria-disabled", "true");

      // 2. Second activation cycles checked -> unchecked ("false")
      const cellChecked = container.querySelector('[data-row-id$="r1"][data-col-id$="data:flag"]')!;
      fireEvent.doubleClick(cellChecked);
      const editor2 = (await screen.findByLabelText("edit flag")) as HTMLInputElement;
      fireEvent.click(editor2);
      fireEvent.blur(editor2);
      await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(2));
      expect(currentRows[0].data.flag).toBe("false");

      rerender(
        <LabelGrid
          rows={currentRows}
          fields={["flag"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );
      const restingBox2 = screen.getByRole("checkbox", { name: /flag unchecked/i }) as HTMLInputElement;
      expect(restingBox2).not.toBeChecked();
      expect(restingBox2).toHaveAttribute("aria-disabled", "true");

      // 3. Third activation cycles unchecked -> unset ("")
      const cellUnchecked = container.querySelector('[data-row-id$="r1"][data-col-id$="data:flag"]')!;
      fireEvent.doubleClick(cellUnchecked);
      const editor3 = (await screen.findByLabelText("edit flag")) as HTMLInputElement;
      fireEvent.click(editor3);
      fireEvent.blur(editor3);
      await waitFor(() => expect(onRowsChange).toHaveBeenCalledTimes(3));
      expect(currentRows[0].data.flag).toBe("");

      rerender(
        <LabelGrid
          rows={currentRows}
          fields={["flag"]}
          cellInput={cellInput}
          onRowsChange={onRowsChange}
          onDuplicate={() => {}}
          onRemove={() => {}}
        />,
      );
      const restingBox3 = screen.getByRole("checkbox", { name: /flag unset/i }) as HTMLInputElement;
      expect(restingBox3).not.toBeChecked();
      expect(restingBox3.indeterminate).toBe(true);
      expect(restingBox3).toHaveAttribute("aria-disabled", "true");

      // Row submits no key once unset
      const pruned = pruneDataForSubmit(currentRows[0].data, [spec]);
      expect(pruned).not.toHaveProperty("flag");
    });
  });
});
