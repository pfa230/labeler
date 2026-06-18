import type { LayoutItem, Options } from "../api/types";

// Best-effort token parse of an interpolation string (NOT validation): `{field}` / `{settings.key}`,
// honoring `{{`/`}}` escapes. Unmatched braces are ignored here (the backend rejects them at render time).
function tokens(s: string): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < s.length) {
    if (s[i] === "{" && s[i + 1] === "{") { i += 2; continue; }
    if (s[i] === "}" && s[i + 1] === "}") { i += 2; continue; }
    if (s[i] === "{") {
      const end = s.indexOf("}", i + 1);
      if (end === -1) break;
      out.push(s.slice(i + 1, end));
      i = end + 1;
      continue;
    }
    i += 1;
  }
  return out;
}

export function defaultOptions(options?: Options): Record<string, string> {
  const sel: Record<string, string> = {};
  for (const [k, vals] of Object.entries(options ?? {})) if (vals[0] !== undefined) sel[k] = vals[0];
  return sel;
}

// Every declared option present, defaulting to its first allowed value; an existing non-empty value for a
// still-declared option is kept (so a CSV value or per-row edit survives), options not declared are dropped.
export function reconcileRowOptions(current: Record<string, string>, options?: Options): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [name, vals] of Object.entries(options ?? {})) {
    out[name] = current[name] ? current[name] : (vals[0] ?? "");
  }
  return out;
}

// A text/qr item carries EXACTLY ONE of name|value (backend invariant). Emit name if present, else value tokens.
function walk(
  items: LayoutItem[],
  selected: Record<string, string>,
  onData: (t: string) => void,
  onImage: (t: string) => void,
) {
  const gating = Object.keys(selected).length > 0; // no selection => mirror backend's "render all" (no gate)
  for (const it of items) {
    if (it.type === "text" || it.type === "qr") {
      if (it.name) onData(it.name);
      else if (it.value) for (const t of tokens(it.value)) onData(t);
    } else if (it.type === "image") {
      // a data-bound image is BOTH a referenced data field AND an image field (sample = data URI)
      if (it.name) { onData(it.name); onImage(it.name); }
    } else if (it.type === "container") {
      const match = !gating || Object.entries(it.option ?? {}).every(([k, v]) => selected[k] === v);
      if (match) walk(it.items, selected, onData, onImage);
    }
  }
}

// Data fields the (option-selected) layout references — text/qr name|value tokens (excluding settings.*).
export function referencedFields(layout: LayoutItem[], selected: Record<string, string>): string[] {
  const set = new Set<string>();
  walk(layout, selected, (t) => { if (!t.startsWith("settings.")) set.add(t); }, () => {});
  return [...set];
}

// Subset of referenced fields that are data-bound IMAGE fields (need a data-URI sample, not text).
export function imageFields(layout: LayoutItem[], selected: Record<string, string>): string[] {
  const set = new Set<string>();
  walk(layout, selected, () => {}, (t) => set.add(t));
  return [...set];
}

// {settings.*} keys referenced anywhere in the layout (not option-gated; discovery across all branches).
export function referencedSettings(layout: LayoutItem[]): string[] {
  const set = new Set<string>();
  const rec = (items: LayoutItem[]) => {
    for (const it of items) {
      if ((it.type === "text" || it.type === "qr") && it.value) {
        for (const t of tokens(it.value)) if (t.startsWith("settings.")) set.add(t.slice("settings.".length));
      } else if (it.type === "container") rec(it.items);
    }
  };
  rec(layout);
  return [...set];
}
