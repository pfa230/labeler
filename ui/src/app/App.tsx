import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { routes } from "./routes";

// Created once per app lifetime: recreating the router on render would rebuild history and drop the
// data-router context the YAML guards rely on.
const router = createBrowserRouter(routes);

export function App() {
  return <RouterProvider router={router} />;
}
