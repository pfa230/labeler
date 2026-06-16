import { defineConfig } from "vitest/config"; // re-exports vite's defineConfig + the `test` field
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  base: "/",
  build: { outDir: "dist" },
  server: {
    proxy: { "/api": { target: "http://localhost:8080", changeOrigin: true } },
  },
  test: { environment: "jsdom", setupFiles: "./src/setupTests.ts", globals: true },
});
