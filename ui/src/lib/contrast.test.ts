import { describe, it, expect } from "vitest";
import { contrastRatio, relativeLuminance } from "./contrast";

// theme.test.ts trusts these numbers to decide whether the palette passes AA, so the formula itself
// is pinned against published WCAG reference ratios rather than against its own output.
describe("contrastRatio", () => {
  it("gives 21:1 for black on white", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 5);
  });

  it("gives the published 4.48:1 for #777777 on white", () => {
    expect(contrastRatio("#777777", "#ffffff")).toBeCloseTo(4.478, 3);
  });

  // Grey exercises the gamma curve but not the channel weights, since all three are equal in it.
  // Pure red and pure blue on white reduce to 1.05 / (weight + 0.05), so these two pin 0.2126 and
  // 0.0722 in place: swapping the weights moves both.
  it("gives the published 4.00:1 for pure red on white", () => {
    expect(contrastRatio("#ff0000", "#ffffff")).toBeCloseTo(3.998, 3);
  });

  it("gives the published 8.59:1 for pure blue on white", () => {
    expect(contrastRatio("#0000ff", "#ffffff")).toBeCloseTo(8.592, 3);
  });

  it("gives 1:1 for a colour against itself", () => {
    expect(contrastRatio("#2f6f7d", "#2f6f7d")).toBeCloseTo(1, 10);
  });

  it("does not depend on the order of its arguments", () => {
    expect(contrastRatio("#b8420f", "#fbe9e2")).toBeCloseTo(contrastRatio("#fbe9e2", "#b8420f"), 10);
  });

  it("is case-insensitive and tolerates surrounding whitespace", () => {
    expect(contrastRatio(" #B8420F ", "#FBE9E2")).toBeCloseTo(contrastRatio("#b8420f", "#fbe9e2"), 10);
  });

  it("refuses a value that is not a 6-digit hex colour", () => {
    expect(() => relativeLuminance("")).toThrow(/6-digit hex/);
    expect(() => relativeLuminance("#fff")).toThrow(/6-digit hex/);
    expect(() => relativeLuminance("var(--info)")).toThrow(/6-digit hex/);
  });
});
