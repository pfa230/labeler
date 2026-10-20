import { describe, expect, it, vi, afterEach } from "vitest";
import { fetchTemplateSource } from "./queries";

describe("fetchTemplateSource", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  // Response.text() eats a leading UTF-8 BOM. The source editor round-trips the fetched text, so a
  // dropped BOM would rewrite the file's first bytes on an untouched field.
  it("retains a leading UTF-8 BOM", async () => {
    const bytes = new Uint8Array([0xef, 0xbb, 0xbf, ...Array.from(new TextEncoder().encode("id: pallet\n"))]);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(bytes, { status: 200 })),
    );

    const source = await fetchTemplateSource("pallet");

    expect(source.charCodeAt(0)).toBe(0xfeff);
    expect(source).toBe("\uFEFFid: pallet\n");
  });

  it("returns the decoded source without a BOM unchanged", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("id: pallet\n", { status: 200 })),
    );

    await expect(fetchTemplateSource("pallet")).resolves.toBe("id: pallet\n");
  });
});
