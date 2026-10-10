import { useState } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ParamInput } from "./ParamInput";
import type { Param, ParamValue } from "../api/types";

describe("ParamInput", () => {
  it("renders a text input for single-line string parameter", () => {
    const onChange = vi.fn();
    const spec: Param = { name: "title", type: "string", control: "text", description: "Title" };
    render(<ParamInput name="title" spec={spec} value="My Label" onChange={onChange} />);

    const input = screen.getByRole("textbox", { name: "Title" }) as HTMLInputElement;
    expect(input).toBeInstanceOf(HTMLInputElement);
    expect(input.value).toBe("My Label");

    fireEvent.change(input, { target: { value: "New Title" } });
    expect(onChange).toHaveBeenCalledWith("New Title");
  });

  it("renders a textarea for multiline string parameter", () => {
    const onChange = vi.fn();
    const spec: Param = { name: "notes", type: "string", control: "textarea", multiline: true, description: "Notes" };
    render(<ParamInput name="notes" spec={spec} value={"Line 1\nLine 2"} onChange={onChange} />);

    const textarea = screen.getByRole("textbox", { name: "Notes" }) as HTMLTextAreaElement;
    expect(textarea).toBeInstanceOf(HTMLTextAreaElement);
    expect(textarea.value).toBe("Line 1\nLine 2");

    fireEvent.change(textarea, { target: { value: "Line 1\nLine 2\nLine 3" } });
    expect(onChange).toHaveBeenCalledWith("Line 1\nLine 2\nLine 3");
  });

  it("renders a file input for an image parameter", async () => {
    const onChange = vi.fn();
    const spec: Param = { name: "logo", type: "string", control: "image", description: "Logo" };
    render(<ParamInput name="logo" spec={spec} value="" onChange={onChange} />);

    const input = screen.getByLabelText("Logo") as HTMLInputElement;
    expect(input.type).toBe("file");
    expect(input.accept).toBe("image/*");

    const file = new File(["fake-image"], "logo.png", { type: "image/png" });
    fireEvent.change(input, { target: { files: [file] } });
    await waitFor(() => expect(onChange).toHaveBeenCalled());
  });

  it("clears the file input selection when value is reset", async () => {
    const onChange = vi.fn();
    const spec: Param = { name: "logo", type: "string", control: "image", description: "Logo" };
    const { rerender } = render(
      <ParamInput name="logo" spec={spec} value="" onChange={onChange} />,
    );

    const input = screen.getByLabelText("Logo") as HTMLInputElement;
    const file = new File(["fake-image"], "logo.png", { type: "image/png" });
    Object.defineProperty(input, "files", { value: [file], configurable: true, writable: true });
    Object.defineProperty(input, "value", { value: "C:\\fakepath\\logo.png", configurable: true, writable: true });

    rerender(<ParamInput name="logo" spec={spec} value="data:image/png;base64,..." onChange={onChange} />);
    expect(screen.getByText("image selected")).toBeInTheDocument();

    rerender(<ParamInput name="logo" spec={spec} value="" onChange={onChange} />);
    expect(screen.queryByText("image selected")).not.toBeInTheDocument();
    expect(input.value).toBe("");
  });

  it("clears a held image back to blank", () => {
    const onChange = vi.fn();
    const spec: Param = { name: "logo", type: "string", control: "image", description: "Logo" };
    render(<ParamInput name="logo" spec={spec} value="data:image/png;base64,AAAA" onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: "clear Logo" }));
    expect(onChange).toHaveBeenCalledWith("");
  });

  it("clears the held image when a selection is cancelled", () => {
    const onChange = vi.fn();
    const spec: Param = { name: "logo", type: "string", control: "image", description: "Logo" };
    render(<ParamInput name="logo" spec={spec} value="data:image/png;base64,AAAA" onChange={onChange} />);
    fireEvent.change(screen.getByLabelText("Logo"), { target: { files: [] } });
    expect(onChange).toHaveBeenCalledWith("");
  });

  it("offers no clear action while no image is held", () => {
    const spec: Param = { name: "logo", type: "string", control: "image", description: "Logo" };
    render(<ParamInput name="logo" spec={spec} value={undefined} onChange={() => {}} />);
    expect(screen.queryByRole("button", { name: "clear Logo" })).not.toBeInTheDocument();
  });

  it("renders a number input when min or max is not specified", () => {
    const onChange = vi.fn();
    const spec: Param = { name: "font_size", type: "number", control: "number", default: 12.5, description: "Font Size" };
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

  it("renders a two-state checkbox for a checkbox control", () => {
    const onChange = vi.fn();
    const spec: Param = { name: "show_border", type: "boolean", control: "checkbox", default: false, description: "Show Border" };
    const { rerender } = render(
      <ParamInput name="show_border" spec={spec} value={false} onChange={onChange} />,
    );

    const checkbox = screen.getByRole("checkbox", { name: "Show Border" }) as HTMLInputElement;
    expect(checkbox.checked).toBe(false);
    expect(checkbox.indeterminate).toBe(false);
    expect(screen.queryByText(/unset|enabled|disabled/i)).toBeNull();

    fireEvent.click(checkbox);
    expect(onChange).toHaveBeenCalledWith(true);

    rerender(<ParamInput name="show_border" spec={spec} value={true} onChange={onChange} />);
    expect(checkbox.checked).toBe(true);
    fireEvent.click(checkbox);
    expect(onChange).toHaveBeenLastCalledWith(false);
  });

  it("renders a select dropdown for enum parameter", () => {
    const onChange = vi.fn();
    const spec: Param = {
      name: "orientation",
      type: "enum",
      control: "select",
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

  it("starts an unset checkbox at its default", () => {
    render(<ParamInput name="bold" spec={{ name: "bold", type: "boolean", control: "checkbox", default: true }} value={undefined} onChange={() => {}} />);
    expect((screen.getByRole("checkbox", { name: "bold" }) as HTMLInputElement).checked).toBe(true);
  });

  it("offers a selectable blank first option naming the default", () => {
    const onChange = vi.fn();
    render(
      <ParamInput
        name="size"
        spec={{ name: "size", type: "enum", control: "select", values: ["small", "large"], default: "large" }}
        value="small"
        onChange={onChange}
      />,
    );
    const select = screen.getByLabelText("size") as HTMLSelectElement;
    const options = [...select.options].map((o) => [o.value, o.textContent, o.disabled]);
    expect(options).toEqual([["", "(default: large)", false], ["small", "small", false], ["large", "large", false]]);
    fireEvent.change(select, { target: { value: "" } });
    expect(onChange).toHaveBeenCalledWith("");
  });

  it("shows the blank option for an unset select with no default", () => {
    render(<ParamInput name="size" spec={{ name: "size", type: "enum", control: "select", values: ["small"] }} value={undefined} onChange={() => {}} />);
    const select = screen.getByLabelText("size") as HTMLSelectElement;
    expect(select.value).toBe("");
    expect(select.options[0].textContent).toBe("");
  });

  it("renders a date input for datetime parameter without time", () => {
    const onChange = vi.fn();
    const spec: Param = { name: "printed_on", type: "datetime", control: "date", description: "Printed Date" };
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
    const spec: Param = { name: "printed_on", type: "datetime", control: "datetime", time: true, description: "Printed Timestamp" };
    render(<ParamInput name="printed_on" spec={spec} value="2026-08-19T14:30" onChange={onChange} />);

    const input = screen.getByLabelText("Printed Timestamp") as HTMLInputElement;
    expect(input).toBeInTheDocument();
    expect(input.type).toBe("datetime-local");
    expect(input.value).toBe("2026-08-19T14:30");

    fireEvent.change(input, { target: { value: "2026-08-19T16:45" } });
    expect(onChange).toHaveBeenCalledWith("2026-08-19T16:45");
  });

  it("renders the list editor, and an undefined value renders zero rows without crashing", () => {
    const onChange = vi.fn();
    const spec: Param = { name: "tags", type: "list", control: "list", description: "Asset Tags" };
    render(<ParamInput name="tags" spec={spec} value={undefined} onChange={onChange} />);
    expect(screen.getByRole("group", { name: "Asset Tags" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByRole("button", { name: "add tags" })).toBeInTheDocument();
  });

  it("appending twice and typing A and B calls onChange with ['A', 'B']; appending one row and typing nothing yields ['']", () => {
    function Stateful(props: { onChange: (v: ParamValue) => void }) {
      const [val, setVal] = useState<ParamValue | undefined>([]);
      return (
        <ParamInput
          name="tags"
          spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
          spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
        spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
        spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
          spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
          spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
          spec={{ name: "tags", type: "list", control: "list", description: "Values" }}
          value={["T1", "T2"]}
          onChange={() => {}}
        />
        <ParamInput
          name="codes"
          spec={{ name: "codes", type: "list", control: "list", description: "Values" }}
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

  it("places focus on the moved element's new row under native event dispatch", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);

    function Stateful() {
      const [val, setVal] = useState<ParamValue | undefined>(["A", "B", "C"]);
      return (
        <ParamInput
          name="tags"
          spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
          spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
          spec={{ name: "tags", type: "list", control: "list", description: "Tags" }}
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
