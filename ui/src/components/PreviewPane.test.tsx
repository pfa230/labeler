import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { PreviewPane } from "./PreviewPane";

describe("PreviewPane", () => {
  it("shows an img for single previews", () => {
    render(<PreviewPane name="L" format="single" preview={{ url: "blob:x", loading: false }} />);
    const img = screen.getByAltText("L preview");
    expect(img.tagName).toBe("IMG");
    expect(img).toHaveAttribute("src", "blob:x");
  });

  it("shows an object for sheet previews", () => {
    render(<PreviewPane name="S" format="sheet" preview={{ url: "blob:y", loading: false }} />);
    expect(screen.getByLabelText("S preview").tagName).toBe("OBJECT");
  });

  it("shows the error line", () => {
    render(<PreviewPane name="L" format="single" preview={{ error: "boom", loading: false }} />);
    expect(screen.getByText("Preview failed: boom")).toBeInTheDocument();
  });
});
