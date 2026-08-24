import type { LayoutItem, Options, ParamSpec, ParamValue } from "../api/types";

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

export function hasServerDefault(spec: ParamSpec): boolean {
  return (
    spec.type === "datetime" ||
    spec.type === "boolean" ||
    (spec.type === "enum" && (spec.values?.length ?? 0) > 0) ||
    spec.default !== undefined
  );
}

export function formatLocalDate(d: Date = new Date()): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function formatLocalDateTime(d: Date = new Date()): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  const h = String(d.getHours()).padStart(2, "0");
  const min = String(d.getMinutes()).padStart(2, "0");
  return `${y}-${m}-${day}T${h}:${min}`;
}

export function initialParamValues(
  params?: Record<string, ParamSpec>,
  now: Date = new Date(),
): Record<string, ParamValue> {
  const data: Record<string, ParamValue> = {};
  if (!params) return data;
  for (const [name, spec] of Object.entries(params)) {
    if (spec.type === "datetime") {
      data[name] = spec.time ? formatLocalDateTime(now) : formatLocalDate(now);
    } else if (spec.default !== undefined && spec.default !== null) {
      data[name] = spec.default;
    } else if (spec.type === "enum" && spec.values && spec.values.length > 0) {
      data[name] = spec.values[0];
    } else if (spec.type === "boolean") {
      data[name] = false;
    }
  }
  return data;
}

export function defaultParamValues(params?: Record<string, ParamSpec>): Record<string, ParamValue> {
  const data: Record<string, ParamValue> = {};
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

export function isLeapYear(year: number): boolean {
  return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
}

export function daysInMonth(year: number, month: number): number {
  if (month === 2) return isLeapYear(year) ? 29 : 28;
  if ([4, 6, 9, 11].includes(month)) return 30;
  return 31;
}

export function datetimeCellError(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed === "") return null;

  // 1. Date only: YYYY-MM-DD
  const dateRegex = /^(\d{4})-(\d{2})-(\d{2})$/;
  // 2. Local date-time: YYYY-MM-DDTHH:MM or YYYY-MM-DDTHH:MM:SS
  const dateTimeRegex = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/;
  // 3. RFC 3339: YYYY-MM-DDTHH:MM:SS(.sss)?(Z|[+-]HH:MM)
  const rfc3339Regex = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(Z|[+-]\d{2}:\d{2})$/;

  let y: number, m: number, d: number;
  let h = 0, min = 0, s = 0;

  const m1 = dateRegex.exec(trimmed);
  if (m1) {
    y = parseInt(m1[1], 10);
    m = parseInt(m1[2], 10);
    d = parseInt(m1[3], 10);
  } else {
    const m2 = dateTimeRegex.exec(trimmed);
    if (m2) {
      y = parseInt(m2[1], 10);
      m = parseInt(m2[2], 10);
      d = parseInt(m2[3], 10);
      h = parseInt(m2[4], 10);
      min = parseInt(m2[5], 10);
      if (m2[6]) s = parseInt(m2[6], 10);
    } else {
      const m3 = rfc3339Regex.exec(trimmed);
      if (m3) {
        y = parseInt(m3[1], 10);
        m = parseInt(m3[2], 10);
        d = parseInt(m3[3], 10);
        h = parseInt(m3[4], 10);
        min = parseInt(m3[5], 10);
        s = parseInt(m3[6], 10);
        const tz = m3[7];
        if (tz !== "Z") {
          const tzH = parseInt(tz.slice(1, 3), 10);
          const tzM = parseInt(tz.slice(4, 6), 10);
          if (tzH > 23 || tzM > 59) {
            return "Invalid timezone offset";
          }
        }
      } else {
        return "Invalid datetime; use YYYY-MM-DD or YYYY-MM-DDTHH:MM";
      }
    }
  }

  if (m < 1 || m > 12) {
    return `Invalid month ${m}; must be 01-12`;
  }
  const maxDays = daysInMonth(y, m);
  if (d < 1 || d > maxDays) {
    return `Invalid day ${d} for month ${m}`;
  }
  if (h < 0 || h > 23) {
    return `Invalid hour ${h}; must be 00-23`;
  }
  if (min < 0 || min > 59) {
    return `Invalid minute ${min}; must be 00-59`;
  }
  if (s < 0 || s > 59) {
    return `Invalid second ${s}; must be 00-59`;
  }

  return null;
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
      if (it.value) for (const t of tokens(it.value)) emit(t);
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
// Declared datetime parameter tokens ({<p>} and {<p>.*}) are likewise excluded from data fields.
const isDataField = (t: string, params?: Record<string, ParamSpec>) => {
  if (t.startsWith("vars.") || t === "datetime" || t.startsWith("datetime.")) {
    return false;
  }
  if (params) {
    const head = t.split(".")[0];
    if (params[head]?.type === "datetime") {
      return false;
    }
  }
  return true;
};

// Data fields the (option-selected) layout references: text/qr value tokens
// (excluding vars.*, datetime, datetime.*, and datetime parameters).
export function referencedFields(
  layout: LayoutItem[],
  selected: Record<string, string>,
  params?: Record<string, ParamSpec>,
): string[] {
  const set = new Set<string>();
  walk(layout, selected, (t) => { if (isDataField(t, params)) set.add(t); }, () => {});
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
export function multilineFields(
  layout: LayoutItem[],
  selected: Record<string, string>,
  params?: Record<string, ParamSpec>,
): string[] {
  const set = new Set<string>();
  walk(layout, selected, () => {}, () => {}, (t, multiline) => {
    if (multiline && isDataField(t, params)) set.add(t);
  });
  return [...set];
}

// The complement: data fields rendered by a single-line text item (a qr payload is neither).
export function singleLineTextFields(
  layout: LayoutItem[],
  selected: Record<string, string>,
  params?: Record<string, ParamSpec>,
): string[] {
  const set = new Set<string>();
  walk(layout, selected, () => {}, () => {}, (t, multiline) => {
    if (!multiline && isDataField(t, params)) set.add(t);
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
