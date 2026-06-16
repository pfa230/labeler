import { describe, it, expect } from "vitest";
import { sampleData } from "./preview";

describe("sampleData", () => {
  it("builds a value per referenced field", () => {
    expect(sampleData(["title", "id"])).toEqual({ title: "title", id: "id" });
  });
});
