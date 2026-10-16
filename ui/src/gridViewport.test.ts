import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// The SVAR grid's own `.wx-grid { height: 100% }` collapses to zero inside an auto-height parent, so
// the bounded height on the class a grid sits in is what makes that grid render at all. The
// ResizeObserver shim in setupTests reports a viewport whatever the stylesheet says, which would
// let every other test pass with the height deleted. These assertions are what stop it masking that.
// import.meta.url is not a file: URL under the jsdom environment, so resolve from the vitest root.
const themeCss = readFileSync(resolve(process.cwd(), "src/theme.css"), "utf8");

// The class may be declared more than once: the shared token-mapping rule carries no height, so the
// height contract is the rule that declares one. Each rule starts a line, with its body ending at
// the first `}`.
function viewportBlock(selector: string): string {
  const rules = [...themeCss.matchAll(new RegExp(`(?:^|\n)${selector}\\s*\\{([^}]*)\\}`, "g"))];
  const block = rules.find(([, body]) => /(^|[;{\s])height:/.test(body));
  expect(block, `no \`${selector} { … height … }\` rule in theme.css`).toBeDefined();
  return block![0];
}

// The batch grid's height is not the shim's, so no constant pairs with it: it is the 350px
// react-data-grid shipped as its own default (lib/styles.css, `block-size: 350px`), kept so that
// moving the grid to SVAR left the region the size it had been (#377). Nothing else in the tree
// records that number.
describe("label grid viewport height contract", () => {
  it("declares the bounded height the batch grid renders in", () => {
    expect(viewportBlock(".label-grid-viewport")).toContain("height: 350px");
  });
});
