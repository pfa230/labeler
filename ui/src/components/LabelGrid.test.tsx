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
  optionNames: [],
  optionValues: {},
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
    { id: "a", origin: "csv", data: { sku: "1" }, option: { color: "red" }, validation: {} },
    {
      id: "b",
      origin: "csv",
      data: { sku: "2" },
      option: { color: "green" },
      validation: { option: { color: "value not allowed" } },
      annotation: { status: "failed", message: "boom" },
    },
  ];
}

const props = {
  fields: ["sku"],
  optionNames: ["color"],
  optionValues: { color: ["red", "blue"] },
};

describe("LabelGrid", () => {
  it("renders data and option cell values", () => {
    render(<LabelGrid rows={rows()} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("red")).toBeInTheDocument();
  });

  it("shows the annotation message for a failed row", () => {
    render(<LabelGrid rows={rows()} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByText(/boom/)).toBeInTheDocument();
  });

  it("shows validation errors: an invalid option and an empty required field", () => {
    const rs: LabelGridRow[] = [
      { id: "a", origin: "csv", data: { sku: "" }, option: { color: "red" }, validation: { field: { sku: "required" } } },
      { id: "b", origin: "csv", data: { sku: "2" }, option: { color: "green" }, validation: { option: { color: "value not allowed" } } },
    ];
    render(<LabelGrid rows={rs} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByLabelText(/sku required/i)).toBeInTheDocument();
    expect(screen.getByTitle(/value not allowed/i)).toBeInTheDocument();
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
    // Double-click the cell to enter edit mode (react-data-grid default), then change the input.
    fireEvent.doubleClick(screen.getByText("1"));
    const input = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "9" } });
    fireEvent.blur(input);
    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[0].data.sku).toBe("9");
  });

  it("commits an option-cell edit (dropdown) through onRowsChange", async () => {
    const onRowsChange = vi.fn();
    render(<LabelGrid rows={rows()} {...props} onRowsChange={onRowsChange} onDuplicate={() => {}} onRemove={() => {}} />);
    fireEvent.doubleClick(screen.getByText("red"));
    const select = (await screen.findByLabelText("edit color")) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "blue" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[0].option.color).toBe("blue");
  });
});
