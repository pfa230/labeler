import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ParamInput } from "./ParamInput";
import type { ParamSpec } from "../api/types";

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
});
