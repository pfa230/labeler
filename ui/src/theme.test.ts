import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { contrastRatio } from "./lib/contrast";

// The badge palette (#201) asserts its own AA compliance from theme.css itself, not from a copy of
// the hex values here: a later palette edit that drops a badge below 4.5:1 has to fail this suite
// rather than drift from a duplicate nobody updated.
//
// import.meta.url is not a file: URL under the jsdom environment, so resolve from the vitest root,
// as pages/connect/connectorGridViewport.test.ts already does.
const css = readFileSync(resolve(process.cwd(), "src/theme.css"), "utf8");

// theme.css puts several declarations on one line, so this scans semicolon-separated declarations
// inside a block rather than lines. `.dark {` is matched with the brace attached so the later
// `.dark .connector-grid-viewport { ... }` block cannot be picked up instead.
function palette(selector: string): Record<string, string> {
  const block = new RegExp(`(?:^|\\n)${selector}\\s*\\{([^}]*)\\}`).exec(css);
  if (!block) throw new Error(`no ${selector} block in theme.css`);
  const tokens: Record<string, string> = {};
  // Comments stripped first: a commented-out token is absent to the browser and must be absent here
  // too, rather than being reported as defined and quietly passing the assertions below.
  for (const declaration of block[1].replace(/\/\*[\s\S]*?\*\//g, "").split(";")) {
    const match = /--([\w-]+)\s*:\s*(\S+)/.exec(declaration);
    // Lower-cased so the comparisons below are between colours, not between spellings of one
    // colour: #2F6F7D and #2f6f7d are the same paint and must not read as two.
    if (match) tokens[match[1]] = match[2].toLowerCase();
  }
  return tokens;
}

const THEMES = { light: palette(":root"), dark: palette("\\.dark") };

// Every token the assertions below reach for. Named up front so a rename fails here, loudly, instead
// of silently skipping the comparison that would have caught it.
const REQUIRED = ["accent", "accent-deep", "accent-soft", "info", "info-soft", "surface", "paper", "good", "bad"];

// What the badge's text and border are painted in, and everything they can sit over: an unselected
// card is --surface, the detail page is --paper, and a selected card is tinted --accent-soft
// (pages/Templates.tsx:63). The cross product includes pairings that do not occur, which is cheaper
// than encoding which are reachable and costs nothing but a few extra passing assertions.
const FOREGROUNDS = ["accent-deep", "info"];
const BACKGROUNDS = ["accent-soft", "info-soft", "surface", "paper"];

describe.each(Object.entries(THEMES))("%s palette", (_theme, tokens) => {
  it("defines every token the badges use", () => {
    expect(Object.keys(tokens)).toEqual(expect.arrayContaining(REQUIRED));
  });

  it.each(FOREGROUNDS)("keeps --%s at 4.5:1 or better over every background it appears on", (fg) => {
    for (const bg of BACKGROUNDS) {
      expect(contrastRatio(tokens[fg], tokens[bg]), `--${fg} on --${bg}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("gives the two badges different text colours and different fills", () => {
    expect(tokens["accent-deep"]).not.toBe(tokens["info"]);
    expect(tokens["accent-soft"]).not.toBe(tokens["info-soft"]);
  });

  it("does not colour the sheet badge as the accent, a success or an error", () => {
    expect(tokens["info"]).not.toBe(tokens["accent"]);
    expect(tokens["info"]).not.toBe(tokens["good"]);
    expect(tokens["info"]).not.toBe(tokens["bad"]);
  });
});
