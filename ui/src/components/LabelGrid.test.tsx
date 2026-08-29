import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { LabelGrid } from "./LabelGrid";
import type { LabelGridRow } from "../lib/labelGrid";

const selectionBaseProps = {
  rows: [
    { id: "r1", origin: "csv" as const, data: { title: "a" }, option: {}, validation: {} },
    { id: "r2", origin: "csv" as const, data: { title: "b" }, option: {}, validation: {} },
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
    { id: "a", origin: "csv", data: { sku: "1", notes: "first" }, option: {}, validation: {} },
    {
      id: "b",
      origin: "csv",
      data: { sku: "2", notes: "second" },
      option: {},
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
      { id: "a", origin: "csv", data: { sku: "", notes: "ok" }, option: {}, validation: { field: { sku: "required" } } },
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
      { id: "a", origin: "csv", data: { sku: "1", notes: "line one\nline two" }, option: {}, validation: {} },
      { id: "b", origin: "csv", data: { sku: "2", notes: "line one line two" }, option: {}, validation: {} },
      { id: "c", origin: "csv", data: { sku: "3", notes: "crlf one\r\ncrlf two" }, option: {}, validation: {} },
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
        option: {},
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
});
