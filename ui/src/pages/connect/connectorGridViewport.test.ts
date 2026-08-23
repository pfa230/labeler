import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { GRID_VIEWPORT_VH, GRID_VIEWPORT_MIN_PX } from "../../setupTests";

// The SVAR grid's own `.wx-grid { height: 100% }` collapses to zero inside an auto-height parent,
// so `.connector-grid-viewport` carrying a bounded height is what makes the grid render at all.
// The ResizeObserver shim in setupTests reports a tall viewport, which would let every other test
// pass even if that height were deleted. These assertions are what stop it masking that.
// import.meta.url is not a file: URL under the jsdom environment, so resolve from the vitest root.
const themeCss = readFileSync(resolve(process.cwd(), "src/theme.css"), "utf8");

function viewportBlock(): string {
  const start = themeCss.indexOf(".connector-grid-viewport {");
  expect(start, ".connector-grid-viewport rule is missing from theme.css").toBeGreaterThan(-1);
  return themeCss.slice(start, themeCss.indexOf("}", start));
}

describe("connector grid viewport height contract", () => {
  it("declares a bounded height, without which the grid renders empty", () => {
    expect(viewportBlock()).toMatch(/(^|[;{]\s*)height:\s*[^;]+/);
  });

  it("declares a min-height floor so the region stays usable on a short screen", () => {
    expect(viewportBlock()).toMatch(/min-height:\s*[^;]+/);
  });

  it("keeps the stylesheet height and the test shim's geometry in step", () => {
    const block = viewportBlock();
    expect(block).toContain(`height: ${GRID_VIEWPORT_VH * 100}vh`);
    expect(block).toContain(`min-height: ${GRID_VIEWPORT_MIN_PX}px`);
  });
});
