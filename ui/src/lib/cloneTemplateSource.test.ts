import { describe, expect, it } from "vitest";
import { cloneTemplateSource } from "./cloneTemplateSource";

describe("cloneTemplateSource", () => {
  it("replaces only a plain name, preserving comments and description verbatim", () => {
    const source = '# header\nname: Pallet  # display name\ndescription: \'Keep this\'\n# dimensions\nwidth: 80\n';
    const expected = '# header\nname: "Pallet (copy)"  # display name\ndescription: \'Keep this\'\n# dimensions\nwidth: 80\n';
    expect(cloneTemplateSource(source, 1)).toBe(expected);
  });

  it("retains the terminator after a block name and leaves the next comment and key intact", () => {
    const source = '# header\nname: >-\n  Pallet\n  label\n# description follows\ndescription: \'Keep this\'\nwidth: 80\n';
    const expected = '# header\nname: "Pallet label (copy 2)"\n# description follows\ndescription: \'Keep this\'\nwidth: 80\n';
    expect(cloneTemplateSource(source, 2)).toBe(expected);
  });
});
