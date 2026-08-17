import type { LayoutItem, Options, ParamSpec } from "../api/types";

// Best-effort token parse of an interpolation string (NOT validation): `{field}` / `{vars.key}`,
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

export function defaultParamValues(params?: Record<string, ParamSpec>): Record<string, any> {
  const data: Record<string, any> = {};
  if (!params) return data;
  for (const [name, spec] of Object.entries(params)) {
    if (spec.default !== undefined && spec.default !== null) {
      data[name] = spec.default;
    } else if (spec.type === "enum" && spec.values && spec.values.length > 0) {
      data[name] = spec.values[0];
    } else if (spec.type === "boolean") {
      data[name] = false;
    }
  }
  return data;
}

export function defaultOptions(options?: Options): Record<string, string> {
  const sel: Record<string, string> = {};
  for (const [k, vals] of Object.entries(options ?? {})) if (vals[0] !== undefined) sel[k] = vals[0];
  return sel;
}

// Every declared option present; an existing value for a still-declared option is kept verbatim (so a CSV
// value or per-row edit survives, including a present-but-blank value, which must stay blank to fail
// validation), options absent from `current` default to their first allowed value, options not declared
// are dropped.
export function reconcileRowOptions(current: Record<string, string>, options?: Options): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [name, vals] of Object.entries(options ?? {})) {
    out[name] = name in current && current[name] !== undefined ? current[name] : (vals[0] ?? "");
  }
  return out;
}

// A text/qr item carries EXACTLY ONE of name|value (backend invariant). Emit name if present, else value tokens.
function walk(
  items: LayoutItem[],
  selected: Record<string, string>,
  onData: (t: string) => void,
  onImage: (t: string) => void,
  onText: (t: string, multiline: boolean) => void = () => {},
) {
  const gating = Object.keys(selected).length > 0; // no selection => mirror backend's "render all" (no gate)
  for (const it of items) {
    if (it.type === "text" || it.type === "qr") {
      const emit = (t: string) => {
        onData(t);
        if (it.type === "text") onText(t, it.multiline === true); // a qr payload is never multiline
      };
      if (it.name) emit(it.name);
      else if (it.value) for (const t of tokens(it.value)) emit(t);
    } else if (it.type === "image") {
      // a data-bound image is BOTH a referenced data field AND an image field (sample = data URI)
      if (it.name) { onData(it.name); onImage(it.name); }
    } else if (it.type === "container") {
      const match = !gating || Object.entries(it.option ?? {}).every(([k, v]) => selected[k] === v);
      if (match) walk(it.items, selected, onData, onImage, onText);
    }
  }
}

// {vars.*}, {datetime} and {datetime.*} resolve server-side; they are never request data fields.
const isDataField = (t: string) => !t.startsWith("vars.") && t !== "datetime" && !t.startsWith("datetime.");

// Data fields the (option-selected) layout references: text/qr name|value tokens
// (excluding vars.*, datetime, and datetime.*).
export function referencedFields(layout: LayoutItem[], selected: Record<string, string>): string[] {
  const set = new Set<string>();
  walk(layout, selected, (t) => { if (isDataField(t)) set.add(t); }, () => {});
  return [...set];
}

// Subset of referenced fields that are data-bound IMAGE fields (need a data-URI sample, not text).
export function imageFields(layout: LayoutItem[], selected: Record<string, string>): string[] {
  const set = new Set<string>();
  walk(layout, selected, () => {}, (t) => set.add(t));
  return [...set];
}

// Data fields whose text item is multiline. Pass the live selection to choose a control; pass `{}` to ask
// "anywhere in this template", which is how the shared-field warning is computed: the form keeps one `data`
// object across option switches, so a value typed under one branch is submitted under another.
export function multilineFields(layout: LayoutItem[], selected: Record<string, string>): string[] {
  const set = new Set<string>();
  walk(layout, selected, () => {}, () => {}, (t, multiline) => {
    if (multiline && isDataField(t)) set.add(t);
  });
  return [...set];
}

// The complement: data fields rendered by a single-line text item (a qr payload is neither).
export function singleLineTextFields(layout: LayoutItem[], selected: Record<string, string>): string[] {
  const set = new Set<string>();
  walk(layout, selected, () => {}, () => {}, (t, multiline) => {
    if (!multiline && isDataField(t)) set.add(t);
  });
  return [...set];
}

// {vars.*} keys referenced anywhere in the layout (not option-gated; discovery across all branches).
export function referencedVariables(layout: LayoutItem[]): string[] {
  const set = new Set<string>();
  const rec = (items: LayoutItem[]) => {
    for (const it of items) {
      if ((it.type === "text" || it.type === "qr") && it.value) {
        for (const t of tokens(it.value)) if (t.startsWith("vars.")) set.add(t.slice("vars.".length));
      } else if (it.type === "container") rec(it.items);
    }
  };
  rec(layout);
  return [...set];
}
