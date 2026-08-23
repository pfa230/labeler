import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { FormatBadge } from "./FormatBadge";
import type { TemplateFormat } from "../api/types";
import { SHEET_ICON, SINGLE_ICON, iconGeometry } from "../setupTests";

const SINGLE: TemplateFormat = { type: "single", width: 80, height: 24 };

function sheet(positions: number): TemplateFormat {
  return {
    type: "sheet",
    paper_width: 210,
    paper_height: 297,
    label_width: 25,
    label_height: 25,
    positions: Array.from({ length: positions }, (_, i) => [i, 0] as [number, number]),
  };
}

function badge(format: TemplateFormat): HTMLElement {
  const { container } = render(<FormatBadge format={format} />);
  return container.querySelector<HTMLElement>(`[data-format="${format.type}"]`)!;
}

describe("FormatBadge", () => {
  it("states the word alone for a single", () => {
    expect(badge(SINGLE).textContent).toBe("single");
  });

  it("states the position count for a sheet", () => {
    expect(badge(sheet(30)).textContent).toBe("sheet · 30");
  });

  // A one-position sheet is still a sheet: it prints through the sheet path, and collapsing it to
  // "single" would say the template is something it is not.
  it("counts a one-position sheet as a sheet", () => {
    expect(badge(sheet(1)).textContent).toBe("sheet · 1");
  });

  it("draws one cell for a single", () => {
    expect(iconGeometry(badge(SINGLE))).toEqual(SINGLE_ICON);
  });

  it("draws a grid of cells for a sheet", () => {
    const rects = Array.from(badge(sheet(30)).querySelectorAll("rect"));
    expect(iconGeometry(badge(sheet(30)))).toEqual(SHEET_ICON);
    expect(rects.length).toBeGreaterThanOrEqual(4);
    expect(new Set(rects.map((r) => r.getAttribute("x"))).size).toBeGreaterThanOrEqual(2);
    expect(new Set(rects.map((r) => r.getAttribute("y"))).size).toBeGreaterThanOrEqual(2);
  });

  // The icon carries no meaning a screen reader user could not get from the word, so it must not be
  // announced and must not reach the tab order.
  it.each([["single", SINGLE], ["sheet", sheet(30)]] as const)("hides the %s icon from assistive technology", (_name, format) => {
    const el = badge(format);
    const svg = el.querySelector("svg")!;
    expect(svg.getAttribute("aria-hidden")).toBe("true");
    expect(svg.getAttribute("focusable")).toBe("false");
    // Rects existing is not an icon being drawn: a 0x0 svg, or fill="none", would leave every
    // geometry assertion green and erase the cue the spec requires.
    expect(Number(svg.getAttribute("width"))).toBeGreaterThan(0);
    expect(Number(svg.getAttribute("height"))).toBeGreaterThan(0);
    expect(svg.getAttribute("fill")).toBe("currentColor");
    // The text is conveyed because nothing overrides it. Without these three, a role plus an
    // aria-label on the badge would have a screen reader announce something else entirely while
    // every assertion above still passed.
    expect(el).not.toHaveAttribute("role");
    expect(el).not.toHaveAttribute("aria-label");
    expect(el).not.toHaveAttribute("aria-labelledby");
    // ...and the badge is not itself hidden, which would announce nothing rather than the wrong thing.
    expect(el).not.toHaveAttribute("aria-hidden");
    expect(el).not.toHaveAttribute("hidden");
  });

  // theme.test.ts proves what these tokens resolve to; nothing there knows which badge uses which.
  // Without this half an implementation could paint the sheet badge --accent and every palette
  // assertion would still pass.
  it("binds a single to the accent tokens", () => {
    const el = badge(SINGLE);
    expect(el.style.color).toBe("var(--accent-deep)");
    expect(el.style.background).toBe("var(--accent-soft)");
  });

  it("binds a sheet to the info tokens", () => {
    const el = badge(sheet(30));
    expect(el.style.color).toBe("var(--info)");
    expect(el.style.background).toBe("var(--info-soft)");
  });

  // The fill cannot be what delineates the chip: a selected card is tinted --accent-soft, which is
  // the single chip's own fill. The border is, so it has to track the foreground on both variants.
  it.each([["single", SINGLE], ["sheet", sheet(30)]] as const)("delineates the %s chip with a border in its own foreground colour", (_name, format) => {
    const el = badge(format);
    expect(el.style.borderColor).toBe(el.style.color);
    // Exact token, not a substring: `border-0` contains "border" and would have passed while the
    // border was zero-width and the chip vanished into a selected card. jsdom loads no stylesheet, so
    // an effective border width cannot be read here; reject the utilities that would cancel it and
    // leave "is it actually drawn" to the browser check.
    expect(el.className.split(/\s+/)).toContain("border");
    expect(el.className).not.toMatch(/\bborder-(0|none)\b/);
  });
});
