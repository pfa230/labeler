import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { ToastProvider } from "./toast";
import { Shell } from "./Shell";

describe("Shell", () => {
  it("renders the nav sections", () => {
    // Shell renders <ToastRegion/> (needs ToastProvider) and NavLinks (need a Router).
    render(
      <ToastProvider>
        <MemoryRouter><Shell /></MemoryRouter>
      </ToastProvider>,
    );
    for (const label of ["Templates", "Print", "Import", "Settings"]) {
      expect(screen.getByRole("link", { name: label })).toBeInTheDocument();
    }
  });
});
