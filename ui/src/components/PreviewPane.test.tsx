import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { PreviewPane } from "./PreviewPane";
import { noBadgeStyling } from "../setupTests";

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

  // The <object> fallback names the format in its link text. #201 leaves it alone: it is a link
  // label for a browser that cannot render the PDF, not a status marker, and it has no count.
  it("keeps the sheet fallback link as plain prose, with no badge", () => {
    const { container } = render(
      <PreviewPane name="S" format="sheet" preview={{ url: "blob:y", loading: false }} />,
    );
    const link = screen.getByText("Open sheet preview");
    expect(link.tagName).toBe("A");
    expect(link).not.toHaveAttribute("data-format");
    expect(link.querySelector("svg")).toBeNull();
    expect(link.textContent).toBe("Open sheet preview");
    expect(noBadgeStyling(link)).toBe(true);
    expect(container.querySelector("[data-format]")).toBeNull();
  });

  it("shows the error line", () => {
    render(<PreviewPane name="L" format="single" preview={{ error: "boom", loading: false }} />);
    expect(screen.getByText("Preview failed: boom")).toBeInTheDocument();
  });
});
