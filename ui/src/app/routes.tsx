import { createRoutesFromElements, Navigate, Route } from "react-router-dom";
import { Shell } from "./Shell";
import { RequireAuth } from "./RequireAuth";
import { RedirectIfAuthed } from "./RedirectIfAuthed";
import { Templates } from "../pages/Templates";
import { TemplateDetail } from "../pages/TemplateDetail";
import { NewTemplate } from "../pages/NewTemplate";
import { Catalog } from "../pages/Catalog";
import { Import } from "../pages/Import";
import { PrintHistory } from "../pages/PrintHistory";
import { Activity } from "../pages/Activity";
import { Settings } from "../pages/Settings";
import { Login } from "../pages/Login";
import { Setup } from "../pages/Setup";

// The single route definition both the browser router (App) and route-level tests build on, so a
// test cannot pass against a copied tree that has drifted from the one the application serves.
export const routes = createRoutesFromElements(
  <>
    <Route element={<RedirectIfAuthed />}>
      <Route path="/login" element={<Login />} />
      <Route path="/setup" element={<Setup />} />
    </Route>
    <Route element={<RequireAuth />}>
      <Route element={<Shell />}>
        <Route index element={<Templates />} />
        <Route path="templates" element={<Navigate to="/" replace />} />
        <Route path="templates/catalog" element={<Catalog />} />
        <Route path="templates/new" element={<NewTemplate />} />
        <Route path="templates/:id" element={<TemplateDetail />} />
        <Route path="import" element={<Import />} />
        <Route path="print-history" element={<PrintHistory />} />
        <Route path="activity" element={<Activity />} />
        <Route path="settings" element={<Settings />} />
      </Route>
    </Route>
    <Route path="*" element={<Navigate to="/" replace />} />
  </>,
);
