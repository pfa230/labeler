import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { GRID_VIEWPORT_VH, GRID_VIEWPORT_MIN_PX } from "./setupTests";

// The SVAR grid's own `.wx-grid { height: 100% }` collapses to zero inside an auto-height parent, so
// the bounded height on the class a grid sits in is what makes that grid render at all. The
// ResizeObserver shim in setupTests reports a tall viewport whatever the stylesheet says, which would
// let every other test pass with either height deleted. These assertions are what stop it masking
// that, and both grids need their own.
// import.meta.url is not a file: URL under the jsdom environment, so resolve from the vitest root.
const themeCss = readFileSync(resolve(process.cwd(), "src/theme.css"), "utf8");

function viewportBlock(selector: string): string {
  // The rule where the class is the whole selector list, so it has to start a line: the two classes
  // also share a token-mapping rule that ends `.connector-grid-viewport, .label-grid-viewport {`,
  // and that one carries no height.
  const start = themeCss.indexOf(`\n${selector} {`);
  expect(start, `a \`${selector} { … }\` rule of its own is missing from theme.css`).toBeGreaterThan(-1);
  return themeCss.slice(start, themeCss.indexOf("}", start));
}

describe("connector grid viewport height contract", () => {
  const block = () => viewportBlock(".connector-grid-viewport");

  it("declares a bounded height, without which the grid renders empty", () => {
    expect(block()).toMatch(/(^|[;{]\s*)height:\s*[^;]+/);
  });

  it("declares a min-height floor so the region stays usable on a short screen", () => {
    expect(block()).toMatch(/min-height:\s*[^;]+/);
  });

  it("keeps the stylesheet height and the test shim's geometry in step", () => {
    expect(block()).toContain(`height: ${GRID_VIEWPORT_VH * 100}vh`);
    expect(block()).toContain(`min-height: ${GRID_VIEWPORT_MIN_PX}px`);
  });
});

// The batch grid's height is not the shim's, so no constant pairs with it: it is the 350px
// react-data-grid shipped as its own default (lib/styles.css, `block-size: 350px`), kept so that
// moving the grid to SVAR left the region the size it had been (#377). Nothing else in the tree
// records that number.
describe("label grid viewport height contract", () => {
  it("declares the bounded height the batch grid renders in", () => {
    expect(viewportBlock(".label-grid-viewport")).toContain("height: 350px");
  });
});
