import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { Shell } from "./Shell";
import { Templates } from "../pages/Templates";
import { TemplateDetail } from "../pages/TemplateDetail";
import { NewTemplate } from "../pages/NewTemplate";
import { Print } from "../pages/Print";
import { Import } from "../pages/Import";
import { Settings } from "../pages/Settings";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Shell />}>
          <Route index element={<Templates />} />
          <Route path="templates" element={<Navigate to="/" replace />} />
          <Route path="templates/new" element={<NewTemplate />} />
          <Route path="templates/:id" element={<TemplateDetail />} />
          <Route path="print" element={<Print />} />
          <Route path="import" element={<Import />} />
          <Route path="settings" element={<Settings />} />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
