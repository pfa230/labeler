import { useState } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ParamInput } from "./ParamInput";
import type { InputSpec, ParamSpec, ParamValue } from "../api/types";

describe("ParamInput", () => {
  it("renders a text input for single-line string parameter", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "string", multiline: false, description: "Title" };
    render(<ParamInput name="title" spec={spec} value="My Label" onChange={onChange} />);

    const input = screen.getByRole("textbox", { name: "Title" }) as HTMLInputElement;
    expect(input).toBeInstanceOf(HTMLInputElement);
    expect(input.value).toBe("My Label");

    fireEvent.change(input, { target: { value: "New Title" } });
    expect(onChange).toHaveBeenCalledWith("New Title");
  });

  it("renders a textarea for multiline string parameter", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "string", multiline: true, description: "Notes" };
    render(<ParamInput name="notes" spec={spec} value={"Line 1\nLine 2"} onChange={onChange} />);

    const textarea = screen.getByRole("textbox", { name: "Notes" }) as HTMLTextAreaElement;
    expect(textarea).toBeInstanceOf(HTMLTextAreaElement);
    expect(textarea.value).toBe("Line 1\nLine 2");

    fireEvent.change(textarea, { target: { value: "Line 1\nLine 2\nLine 3" } });
    expect(onChange).toHaveBeenCalledWith("Line 1\nLine 2\nLine 3");
  });

  it("renders a file input for an image parameter", async () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "string", description: "Logo" };
    render(<ParamInput name="logo" spec={spec} value="" onChange={onChange} isImage={true} />);

    const input = screen.getByLabelText("Logo") as HTMLInputElement;
    expect(input.type).toBe("file");
    expect(input.accept).toBe("image/*");

    const file = new File(["fake-image"], "logo.png", { type: "image/png" });
    fireEvent.change(input, { target: { files: [file] } });
    await waitFor(() => expect(onChange).toHaveBeenCalled());
  });

  it("drops a file read that finishes after the entry was deferred again", async () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "string", description: "Logo" };
    const { rerender } = render(
      <ParamInput name="logo" spec={spec} value="" onChange={onChange} isImage={true} />,
    );

    const input = screen.getByLabelText("Logo") as HTMLInputElement;
    const file = new File(["fake-image"], "logo.png", { type: "image/png" });
    fireEvent.change(input, { target: { files: [file] } });

    // Re-checking "Use default" while the read is in flight: the chooser is cleared and the control
    // disabled before the reader resolves.
    rerender(<ParamInput name="logo" spec={spec} value="" onChange={onChange} isImage={true} disabled={true} />);

    await new Promise((r) => setTimeout(r, 50));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("clears the file input selection when value is reset or disabled", async () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "string", description: "Logo" };
    const { rerender } = render(
      <ParamInput name="logo" spec={spec} value="" onChange={onChange} isImage={true} />,
    );

    const input = screen.getByLabelText("Logo") as HTMLInputElement;
    const file = new File(["fake-image"], "logo.png", { type: "image/png" });
    Object.defineProperty(input, "files", { value: [file], configurable: true, writable: true });
    Object.defineProperty(input, "value", { value: "C:\\fakepath\\logo.png", configurable: true, writable: true });

    rerender(<ParamInput name="logo" spec={spec} value="data:image/png;base64,..." onChange={onChange} isImage={true} />);
    expect(screen.getByText("image selected")).toBeInTheDocument();

    // Now reset value and disable (simulating re-checking the Use default checkbox)
    rerender(<ParamInput name="logo" spec={spec} value="" onChange={onChange} isImage={true} disabled={true} />);
    expect(screen.queryByText("image selected")).not.toBeInTheDocument();
    expect(input.value).toBe("");
  });

  it("renders a slider for length parameter with min and max bounds", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = {
      type: "length",
      default: 80,
      min: 25,
      max: 200,
      description: "Target Width",
    };
    render(<ParamInput name="target_width" spec={spec} value={100} onChange={onChange} unit="mm" />);

    const slider = screen.getByRole("slider", { name: "Target Width" }) as HTMLInputElement;
    expect(slider).toBeInTheDocument();
    expect(slider.min).toBe("25");
    expect(slider.max).toBe("200");
    expect(slider.value).toBe("100");
    expect(screen.getByText("100 mm")).toBeInTheDocument();

    fireEvent.change(slider, { target: { value: "150" } });
    expect(onChange).toHaveBeenCalledWith(150);
  });

  it("renders an integer slider with step 1 and converts to integer", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = {
      type: "integer",
      default: 400,
      min: 100,
      max: 900,
      description: "Font Weight",
    };
    render(<ParamInput name="weight" spec={spec} value={400} onChange={onChange} />);

    const slider = screen.getByRole("slider", { name: "Font Weight" }) as HTMLInputElement;
    expect(slider.step).toBe("1");

    fireEvent.change(slider, { target: { value: "700" } });
    expect(onChange).toHaveBeenCalledWith(700);
  });

  it("renders a number input when min or max is not specified", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "number", default: 12.5, description: "Font Size" };
    render(<ParamInput name="font_size" spec={spec} value={12.5} onChange={onChange} />);

    const numInput = screen.getByRole("spinbutton", { name: "Font Size" }) as HTMLInputElement;
    expect(numInput).toBeInTheDocument();
    expect(numInput.type).toBe("number");
    expect(numInput.value).toBe("12.5");

    fireEvent.change(numInput, { target: { value: "16.5" } });
    expect(onChange).toHaveBeenCalledWith(16.5);

    fireEvent.change(numInput, { target: { value: "" } });
    expect(onChange).toHaveBeenCalledWith("");
  });

  it("renders a checkbox toggle for boolean parameter", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "boolean", default: false, description: "Show Border" };
    const { rerender } = render(
      <ParamInput name="show_border" spec={spec} value={false} onChange={onChange} />,
    );

    const checkbox = screen.getByRole("checkbox", { name: "Show Border" }) as HTMLInputElement;
    expect(checkbox).toBeInTheDocument();
    expect(checkbox.checked).toBe(false);
    expect(screen.getByText("Disabled")).toBeInTheDocument();

    fireEvent.click(checkbox);
    expect(onChange).toHaveBeenCalledWith(true);

    rerender(<ParamInput name="show_border" spec={spec} value={true} onChange={onChange} />);
    expect(checkbox.checked).toBe(true);
    expect(screen.getByText("Enabled")).toBeInTheDocument();
  });

  it("renders a select dropdown for enum parameter", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = {
      type: "enum",
      values: ["horizontal", "vertical"],
      default: "horizontal",
      description: "Orientation",
    };
    render(<ParamInput name="orientation" spec={spec} value="horizontal" onChange={onChange} />);

    const select = screen.getByRole("combobox", { name: "Orientation" }) as HTMLSelectElement;
    expect(select).toBeInTheDocument();
    expect(select.value).toBe("horizontal");
    expect(screen.getByRole("option", { name: "horizontal" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "vertical" })).toBeInTheDocument();

    fireEvent.change(select, { target: { value: "vertical" } });
    expect(onChange).toHaveBeenCalledWith("vertical");
  });

  it("sets aria-invalid when invalid prop is true", () => {
    const spec: ParamSpec = { type: "string", description: "Required Field" };
    render(
      <ParamInput name="req" spec={spec} value="" onChange={vi.fn()} invalid={true} />,
    );

    const input = screen.getByRole("textbox", { name: "Required Field" });
    expect(input).toHaveAttribute("aria-invalid", "true");
  });

  it("disables the input when disabled prop is true", () => {
    const spec: ParamSpec = { type: "string", description: "Disabled Field" };
    render(
      <ParamInput name="dis" spec={spec} value="" onChange={vi.fn()} disabled={true} />,
    );

    const input = screen.getByRole("textbox", { name: "Disabled Field" });
    expect(input).toBeDisabled();
  });

  it("renders a date input for datetime parameter without time", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "datetime", description: "Printed Date" };
    render(<ParamInput name="printed_on" spec={spec} value="2026-08-19" onChange={onChange} />);

    const input = screen.getByLabelText("Printed Date") as HTMLInputElement;
    expect(input).toBeInTheDocument();
    expect(input.type).toBe("date");
    expect(input.value).toBe("2026-08-19");

    fireEvent.change(input, { target: { value: "2026-08-20" } });
    expect(onChange).toHaveBeenCalledWith("2026-08-20");
  });

  it("renders a datetime-local input for datetime parameter with time", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "datetime", time: true, description: "Printed Timestamp" };
    render(<ParamInput name="printed_on" spec={spec} value="2026-08-19T14:30" onChange={onChange} />);

    const input = screen.getByLabelText("Printed Timestamp") as HTMLInputElement;
    expect(input).toBeInTheDocument();
    expect(input.type).toBe("datetime-local");
    expect(input.value).toBe("2026-08-19T14:30");

    fireEvent.change(input, { target: { value: "2026-08-19T16:45" } });
    expect(onChange).toHaveBeenCalledWith("2026-08-19T16:45");
  });

  it("renders an unset checkbox when value and default are undefined", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "boolean", description: "Flag" };
    render(<ParamInput name="flag" spec={spec} value={undefined} onChange={onChange} />);

    const checkbox = screen.getByRole("checkbox", { name: "Flag" }) as HTMLInputElement;
    expect(checkbox).toBeInTheDocument();
    expect(checkbox.checked).toBe(false);
    expect(screen.getByText("Unset")).toBeInTheDocument();
  });

  it("renders a select with placeholder when value and default are undefined", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "enum", values: ["a", "b"], description: "Choice" };
    render(<ParamInput name="choice" spec={spec} value={undefined} onChange={onChange} />);

    const select = screen.getByRole("combobox", { name: "Choice" }) as HTMLSelectElement;
    expect(select).toBeInTheDocument();
    expect(select.value).toBe("");
    expect(screen.getByText("Select...")).toBeInTheDocument();
  });

  it("renders a number input rather than a slider when bounds are set but default is missing", () => {
    const onChange = vi.fn();
    const spec: ParamSpec = { type: "length", min: 10, max: 100, description: "Width" };
    render(<ParamInput name="width" spec={spec} value={undefined} onChange={onChange} />);

    expect(screen.queryByRole("slider")).toBeNull();
    const spin = screen.getByRole("spinbutton", { name: "Width" }) as HTMLInputElement;
    expect(spin).toBeInTheDocument();
  });

  it("does not substitute a default for an unset checkbox when InputSpec carries one", () => {
    const onChange = vi.fn();
    const spec = { name: "flag", control: "checkbox", required: false, default: true } as unknown as ParamSpec;
    render(<ParamInput name="flag" spec={spec} value={undefined} onChange={onChange} />);

    const checkbox = screen.getByRole("checkbox", { name: "flag" }) as HTMLInputElement;
    expect(checkbox.checked).toBe(false);
    expect(screen.getByText("Unset")).toBeInTheDocument();
  });

  it("does not substitute a default for an unset select when InputSpec carries one", () => {
    const onChange = vi.fn();
    const spec = { name: "choice", control: "select", values: ["a", "b"], required: false, default: "a" } as unknown as ParamSpec;
    render(<ParamInput name="choice" spec={spec} value={undefined} onChange={onChange} />);

    const select = screen.getByRole("combobox", { name: "choice" }) as HTMLSelectElement;
    expect(select.value).toBe("");
    expect(screen.getByText("Select...")).toBeInTheDocument();
  });

  it("renders editor for control 'list' and ParamSpec type 'list', and undefined value renders zero rows without crashing", () => {
    const onChange = vi.fn();
    const inputSpec: InputSpec = { name: "tags", control: "list", description: "Asset Tags", required: true };
    const { rerender } = render(
      <ParamInput name="tags" spec={inputSpec} value={undefined} onChange={onChange} />,
    );
    expect(screen.getByRole("group", { name: "Asset Tags" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByRole("button", { name: "add tags" })).toBeInTheDocument();

    const paramSpec: ParamSpec = { type: "list", description: "Tags" };
    rerender(<ParamInput name="tags" spec={paramSpec} value={undefined} onChange={onChange} />);
    expect(screen.getByRole("group", { name: "Tags" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByRole("button", { name: "add tags" })).toBeInTheDocument();
  });

  it("appending twice and typing A and B calls onChange with ['A', 'B']; appending one row and typing nothing yields ['']", () => {
    function Stateful(props: { onChange: (v: ParamValue) => void }) {
      const [val, setVal] = useState<ParamValue | undefined>([]);
      return (
        <ParamInput
          name="tags"
          spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
          value={val}
          onChange={(v) => {
            setVal(v);
            props.onChange(v);
          }}
        />
      );
    }

    const onChange = vi.fn();
    const { unmount } = render(<Stateful onChange={onChange} />);

    const addBtn = screen.getByRole("button", { name: "add tags" });
    fireEvent.click(addBtn);
    expect(onChange).toHaveBeenLastCalledWith([""]);

    const input1 = screen.getByRole("textbox", { name: "tags 1" });
    fireEvent.change(input1, { target: { value: "A" } });
    expect(onChange).toHaveBeenLastCalledWith(["A"]);

    fireEvent.click(addBtn);
    expect(onChange).toHaveBeenLastCalledWith(["A", ""]);

    const input2 = screen.getByRole("textbox", { name: "tags 2" });
    fireEvent.change(input2, { target: { value: "B" } });
    expect(onChange).toHaveBeenLastCalledWith(["A", "B"]);

    unmount();

    // Appending one row and typing nothing yields [""]
    const onChangeSingle = vi.fn();
    render(<Stateful onChange={onChangeSingle} />);
    const addBtnSingle = screen.getByRole("button", { name: "add tags" });
    fireEvent.click(addBtnSingle);
    expect(onChangeSingle).toHaveBeenCalledWith([""]);
    expect(screen.getAllByRole("textbox")).toHaveLength(1);
    expect((screen.getByRole("textbox", { name: "tags 1" }) as HTMLInputElement).value).toBe("");
  });

  it("moves and removes elements in row order", () => {
    function Stateful(props: { initial: string[]; onChange: (v: ParamValue) => void }) {
      const [val, setVal] = useState<ParamValue | undefined>(props.initial);
      return (
        <ParamInput
          name="tags"
          spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
          value={val}
          onChange={(v) => {
            setVal(v);
            props.onChange(v);
          }}
        />
      );
    }

    const onChangeMove = vi.fn();
    const { unmount } = render(<Stateful initial={["A", "B", "C"]} onChange={onChangeMove} />);

    // Move C (position 3) one position earlier -> ["A", "C", "B"]
    fireEvent.click(screen.getByRole("button", { name: "move tags 3 earlier" }));
    expect(onChangeMove).toHaveBeenLastCalledWith(["A", "C", "B"]);

    // Move A (now position 1) one position later -> ["C", "A", "B"]
    fireEvent.click(screen.getByRole("button", { name: "move tags 1 later" }));
    expect(onChangeMove).toHaveBeenLastCalledWith(["C", "A", "B"]);

    unmount();

    // With A, B, C: removing B (position 2) yields ["A", "C"]
    const onChangeRemove = vi.fn();
    render(<Stateful initial={["A", "B", "C"]} onChange={onChangeRemove} />);
    fireEvent.click(screen.getByRole("button", { name: "remove tags 2" }));
    expect(onChangeRemove).toHaveBeenLastCalledWith(["A", "C"]);
  });

  it("inert move controls at boundaries report unavailable, do not call onChange, and remain reachable by keyboard", () => {
    const onChange = vi.fn();
    render(
      <ParamInput
        name="tags"
        spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
        value={["A", "B", "C"]}
        onChange={onChange}
      />,
    );

    const firstEarlier = screen.getByRole("button", { name: "move tags 1 earlier" });
    const lastLater = screen.getByRole("button", { name: "move tags 3 later" });

    // Report themselves unavailable
    expect(firstEarlier).toHaveAttribute("aria-disabled", "true");
    expect(lastLater).toHaveAttribute("aria-disabled", "true");

    // Reachable by keyboard (not natively disabled and focusable)
    expect(firstEarlier).not.toBeDisabled();
    expect(lastLater).not.toBeDisabled();
    firstEarlier.focus();
    expect(document.activeElement).toBe(firstEarlier);
    lastLater.focus();
    expect(document.activeElement).toBe(lastLater);

    // Activating either calls no onChange
    fireEvent.click(firstEarlier);
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.click(lastLater);
    expect(onChange).not.toHaveBeenCalled();

    // The other four move controls each move an element
    const firstLater = screen.getByRole("button", { name: "move tags 1 later" });
    expect(firstLater).not.toHaveAttribute("aria-disabled");
    expect(firstLater).not.toBeDisabled();
    fireEvent.click(firstLater);
    expect(onChange).toHaveBeenLastCalledWith(["B", "A", "C"]);

    onChange.mockClear();
    const secondEarlier = screen.getByRole("button", { name: "move tags 2 earlier" });
    expect(secondEarlier).not.toHaveAttribute("aria-disabled");
    expect(secondEarlier).not.toBeDisabled();
    fireEvent.click(secondEarlier);
    expect(onChange).toHaveBeenLastCalledWith(["B", "A", "C"]);

    onChange.mockClear();
    const secondLater = screen.getByRole("button", { name: "move tags 2 later" });
    expect(secondLater).not.toHaveAttribute("aria-disabled");
    expect(secondLater).not.toBeDisabled();
    fireEvent.click(secondLater);
    expect(onChange).toHaveBeenLastCalledWith(["A", "C", "B"]);

    onChange.mockClear();
    const thirdEarlier = screen.getByRole("button", { name: "move tags 3 earlier" });
    expect(thirdEarlier).not.toHaveAttribute("aria-disabled");
    expect(thirdEarlier).not.toBeDisabled();
    fireEvent.click(thirdEarlier);
    expect(onChange).toHaveBeenLastCalledWith(["A", "C", "B"]);
  });

  it("with a single element, both move controls report aria-disabled and activating either calls no onChange", () => {
    const onChange = vi.fn();
    render(
      <ParamInput
        name="tags"
        spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
        value={["A"]}
        onChange={onChange}
      />,
    );

    const upBtn = screen.getByRole("button", { name: "move tags 1 earlier" });
    const downBtn = screen.getByRole("button", { name: "move tags 1 later" });

    expect(upBtn).toHaveAttribute("aria-disabled", "true");
    expect(upBtn).not.toBeDisabled();
    expect(downBtn).toHaveAttribute("aria-disabled", "true");
    expect(downBtn).not.toBeDisabled();

    upBtn.focus();
    expect(document.activeElement).toBe(upBtn);
    downBtn.focus();
    expect(document.activeElement).toBe(downBtn);

    fireEvent.click(upBtn);
    fireEvent.click(downBtn);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("leaves focus on the first row's inert move-earlier control after moving second element earlier, and activating it again calls no onChange", () => {
    function Stateful(props: { onChange: (v: ParamValue) => void }) {
      const [val, setVal] = useState<ParamValue | undefined>(["A", "B", "C"]);
      return (
        <ParamInput
          name="tags"
          spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
          value={val}
          onChange={(v) => {
            setVal(v);
            props.onChange(v);
          }}
        />
      );
    }

    const onChange = vi.fn();
    render(<Stateful onChange={onChange} />);

    const secondEarlier = screen.getByRole("button", { name: "move tags 2 earlier" });
    secondEarlier.focus();
    expect(document.activeElement).toBe(secondEarlier);

    fireEvent.click(secondEarlier);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith(["B", "A", "C"]);

    const firstEarlier = screen.getByRole("button", { name: "move tags 1 earlier" });
    expect(document.activeElement).toBe(firstEarlier);
    expect(firstEarlier).toHaveAttribute("aria-disabled", "true");

    fireEvent.click(firstEarlier);
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("places focus correctly after removals", () => {
    function Stateful(props: { initial: string[]; onChange: (v: ParamValue) => void }) {
      const [val, setVal] = useState<ParamValue | undefined>(props.initial);
      return (
        <ParamInput
          name="tags"
          spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
          value={val}
          onChange={(v) => {
            setVal(v);
            props.onChange(v);
          }}
        />
      );
    }

    // Case 1: Removing middle of three rows leaves focus on removing control of row that took its place
    const onChange1 = vi.fn();
    const { unmount: unmount1 } = render(<Stateful initial={["A", "B", "C"]} onChange={onChange1} />);
    fireEvent.click(screen.getByRole("button", { name: "remove tags 2" }));
    expect(onChange1).toHaveBeenCalledWith(["A", "C"]);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "remove tags 2" }));
    unmount1();

    // Case 2: Removing last of two leaves focus on preceding row's removing control
    const onChange2 = vi.fn();
    const { unmount: unmount2 } = render(<Stateful initial={["A", "B"]} onChange={onChange2} />);
    fireEvent.click(screen.getByRole("button", { name: "remove tags 2" }));
    expect(onChange2).toHaveBeenCalledWith(["A"]);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "remove tags 1" }));
    unmount2();

    // Case 3: Removing the only row leaves focus on appending control
    const onChange3 = vi.fn();
    render(<Stateful initial={["A"]} onChange={onChange3} />);
    fireEvent.click(screen.getByRole("button", { name: "remove tags 1" }));
    expect(onChange3).toHaveBeenCalledWith([]);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "add tags" }));
  });

  it("gives every control an accessible name containing entry name and element position", () => {
    render(
      <div>
        <ParamInput
          name="tags"
          spec={{ control: "list", description: "Values", required: true, name: "tags" }}
          value={["T1", "T2"]}
          onChange={() => {}}
        />
        <ParamInput
          name="codes"
          spec={{ control: "list", description: "Values", required: true, name: "codes" }}
          value={["C1", "C2"]}
          onChange={() => {}}
        />
      </div>,
    );

    // tags controls
    expect(screen.getByRole("textbox", { name: "tags 1" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "tags 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "move tags 1 earlier" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "move tags 1 later" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "move tags 2 earlier" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "move tags 2 later" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "remove tags 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "remove tags 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "add tags" })).toBeInTheDocument();

    // codes controls
    expect(screen.getByRole("textbox", { name: "codes 1" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "codes 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "move codes 1 earlier" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "move codes 1 later" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "move codes 2 earlier" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "move codes 2 later" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "remove codes 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "remove codes 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "add codes" })).toBeInTheDocument();
  });

  it("disables every control in the editor when disabled is true, while showing row values", () => {
    render(
      <ParamInput
        name="tags"
        spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
        value={["ALPHA", "BETA"]}
        disabled={true}
        onChange={() => {}}
      />,
    );

    const input1 = screen.getByRole("textbox", { name: "tags 1" }) as HTMLInputElement;
    const input2 = screen.getByRole("textbox", { name: "tags 2" }) as HTMLInputElement;
    expect(input1.value).toBe("ALPHA");
    expect(input2.value).toBe("BETA");
    expect(input1).toBeDisabled();
    expect(input2).toBeDisabled();

    expect(screen.getByRole("button", { name: "move tags 1 earlier" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "move tags 1 later" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "move tags 2 earlier" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "move tags 2 later" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "remove tags 1" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "remove tags 2" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "add tags" })).toBeDisabled();
  });

  it("places focus on the moved element's new row under native event dispatch", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);

    function Stateful() {
      const [val, setVal] = useState<ParamValue | undefined>(["A", "B", "C"]);
      return (
        <ParamInput
          name="tags"
          spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
          value={val}
          onChange={(v) => setVal(v)}
        />
      );
    }

    try {
      flushSync(() => {
        root.render(<Stateful />);
      });

      const move2Earlier = container.querySelector('button[aria-label="move tags 2 earlier"]') as HTMLButtonElement;
      expect(move2Earlier).not.toBeNull();
      move2Earlier.focus();
      move2Earlier.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));

      await Promise.resolve();

      const activeBtn = document.activeElement as HTMLButtonElement;
      expect(activeBtn?.getAttribute("aria-label")).toBe("move tags 1 earlier");
    } finally {
      flushSync(() => {
        root.unmount();
      });
      container.remove();
    }
  });

  it("places focus on the removing control of the row taking its place under native event dispatch", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);

    function Stateful() {
      const [val, setVal] = useState<ParamValue | undefined>(["A", "B", "C"]);
      return (
        <ParamInput
          name="tags"
          spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
          value={val}
          onChange={(v) => setVal(v)}
        />
      );
    }

    try {
      flushSync(() => {
        root.render(<Stateful />);
      });

      const remove2 = container.querySelector('button[aria-label="remove tags 2"]') as HTMLButtonElement;
      expect(remove2).not.toBeNull();
      remove2.focus();
      remove2.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));

      await Promise.resolve();

      const activeBtn = document.activeElement as HTMLButtonElement;
      expect(activeBtn?.getAttribute("aria-label")).toBe("remove tags 2");
    } finally {
      flushSync(() => {
        root.unmount();
      });
      container.remove();
    }
  });

  it("places focus on the append button when removing the only row under native event dispatch", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);

    function Stateful() {
      const [val, setVal] = useState<ParamValue | undefined>(["A"]);
      return (
        <ParamInput
          name="tags"
          spec={{ control: "list", description: "Tags", required: true, name: "tags" }}
          value={val}
          onChange={(v) => setVal(v)}
        />
      );
    }

    try {
      flushSync(() => {
        root.render(<Stateful />);
      });

      const remove1 = container.querySelector('button[aria-label="remove tags 1"]') as HTMLButtonElement;
      expect(remove1).not.toBeNull();
      remove1.focus();
      remove1.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));

      await Promise.resolve();

      const activeBtn = document.activeElement as HTMLButtonElement;
      expect(activeBtn?.getAttribute("aria-label")).toBe("add tags");
    } finally {
      flushSync(() => {
        root.unmount();
      });
      container.remove();
    }
  });
});
