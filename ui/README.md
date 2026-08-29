# labeler UI

React + TypeScript SPA (Vite, Tailwind) for the labeler service. It consumes the JSON REST API under
`/api`; the Rust backend serves this app's build (`ui/dist`) at `/`.

```bash
npm install            # once
npm run dev            # Vite dev server; proxies /api -> http://localhost:8080 (run `cargo run` too)
npm run build          # -> ui/dist (served by the backend at /)
npm run test           # vitest
npm run lint           # eslint
```

npm is the only supported package manager, and `ui/package-lock.json` is the only lockfile. The
`packageManager` field in `package.json` makes `pnpm install` exit 1 here instead of quietly writing a
second one.

Structure: `src/api/` (typed `/api` client + TanStack Query hooks), `src/app/` (Shell, routing, theme,
toasts), `src/pages/` (screens). Theme tokens ("Ink & Tape") live in `src/theme.css`; the dark class is
applied pre-paint by an inline script in `index.html`.
