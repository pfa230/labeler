import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { Shell } from "./Shell";
import { RequireAuth } from "./RequireAuth";
import { Templates } from "../pages/Templates";
import { TemplateDetail } from "../pages/TemplateDetail";
import { NewTemplate } from "../pages/NewTemplate";
import { Print } from "../pages/Print";
import { Import } from "../pages/Import";
import { Connect } from "../pages/Connect";
import { Settings } from "../pages/Settings";
import { Login } from "../pages/Login";
import { Setup } from "../pages/Setup";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<Login />} />
        <Route path="/setup" element={<Setup />} />
        <Route element={<RequireAuth />}>
          <Route element={<Shell />}>
            <Route index element={<Templates />} />
            <Route path="templates" element={<Navigate to="/" replace />} />
            <Route path="templates/new" element={<NewTemplate />} />
            <Route path="templates/:id" element={<TemplateDetail />} />
            <Route path="print" element={<Print />} />
            <Route path="import" element={<Import />} />
            <Route path="connect" element={<Connect />} />
            <Route path="settings" element={<Settings />} />
          </Route>
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
