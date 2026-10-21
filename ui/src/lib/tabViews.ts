import { useEffect, useRef } from "react";
import { useLocation } from "react-router-dom";
import type { LabelGridRow } from "./labelGrid";

interface ImportDraft {
  text: string;
  loadedSource: string;
  rows: Pick<LabelGridRow, "id" | "data">[];
  csvFields: string[];
  issues: string[];
  copies: number;
  startSlot: number;
  printer?: string;
  selectedRowId?: string;
}

export function readImportDraft(): ImportDraft | null {
  const saved = sessionStorage.getItem("labeler:import-draft");
  return saved ? JSON.parse(saved) : null;
}

export function saveImportDraft(draft: ImportDraft) {
  sessionStorage.setItem("labeler:import-draft", JSON.stringify({
    ...draft,
    rows: draft.rows.map(({ id, data }) => ({ id, data })),
  }));
}

export function rememberedDestination(path: string): string {
  return sessionStorage.getItem(`labeler:destination:${path}`) ?? path;
}

export function rememberDestination(path: string, search: string) {
  sessionStorage.setItem(`labeler:destination:${path}`, path + search);
}

interface ViewScroll {
  top: number;
  left: number;
}

export function readViewScroll(scroller: "main" | "grid", url: string): ViewScroll {
  const saved = sessionStorage.getItem(`labeler:scroll:${scroller}:${url}`);
  return saved ? JSON.parse(saved) : { top: 0, left: 0 };
}

export function saveViewScroll(scroller: "main" | "grid", url: string, position: ViewScroll) {
  sessionStorage.setItem(`labeler:scroll:${scroller}:${url}`, JSON.stringify(position));
}

// Screens signal when their content is mounted; the shell's main element is the scroller.
export function useMainScroll(ready: boolean) {
  const root = useRef<HTMLDivElement>(null);
  const { pathname, search } = useLocation();
  const url = pathname + search;
  useEffect(() => {
    const content = root.current;
    const main = content?.closest("main");
    if (!ready || !content || !main) return;
    const position = readViewScroll("main", url);
    const save = () => {
      // Scrolling takes over from a pending restore, even if its offset no longer fits.
      observer.disconnect();
      saveViewScroll("main", url, { top: main.scrollTop, left: main.scrollLeft });
    };
    const observer = new ResizeObserver(() => restore());
    const restore = () => {
      // The asynchronous preview can still be expanding the destination page.
      if (main.scrollHeight - main.clientHeight < position.top || main.scrollWidth - main.clientWidth < position.left) return;
      main.scrollTop = position.top;
      main.scrollLeft = position.left;
      observer.disconnect();
    };
    // Run after mount effects, including the grid's initial focus.
    const frame = requestAnimationFrame(() => {
      observer.observe(content);
      restore();
      main.addEventListener("scroll", save);
    });
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      main.removeEventListener("scroll", save);
    };
  }, [ready, url]);
  return root;
}
