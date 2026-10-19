import { defineConfig } from "vitest/config"; // re-exports vite's defineConfig + the `test` field
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "/",
  build: { outDir: "dist", cssTarget: "esnext" },
  server: {
    proxy: { "/api": { target: "http://localhost:8080", changeOrigin: true } },
  },
  test: { environment: "jsdom", setupFiles: "./src/setupTests.ts", globals: true },
});
