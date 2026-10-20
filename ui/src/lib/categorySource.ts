import { isMap, isScalar, isSeq, parseDocument } from "yaml";

const INVALID_SOURCE = "The template source is not valid YAML.";
const UNSUPPORTED_SOURCE =
  "This template's categories cannot be edited here because they are not a direct list.";

export type CategoryRead = { ok: true; categories: string[] } | { ok: false; reason: string };
export type CategoryEdit = { ok: true; source: string } | { ok: false; reason: string };

type Located =
  | { kind: "replace"; categories: string[]; start: number; end: number; newline: string }
  | { kind: "insert"; at: number; indent: string; flow: boolean; newline: string; categories: string[] }
  | { kind: "unsupported"; reason: string };

function lineTerminator(source: string): string {
  return source.includes("\r\n") ? "\r\n" : "\n";
}

function indentationAt(source: string, position: number): string {
  const lineStart = source.lastIndexOf("\n", position - 1) + 1;
  let end = lineStart;
  while (end < position && (source[end] === " " || source[end] === "\t")) end++;
  return source.slice(lineStart, end);
}

function locate(source: string): Located {
  const doc = parseDocument(source);
  if (doc.errors.length > 0) return { kind: "unsupported", reason: INVALID_SOURCE };

  const root = doc.contents;
  if (!isMap(root) || !root.range || root.anchor || root.tag) {
    return { kind: "unsupported", reason: UNSUPPORTED_SOURCE };
  }

  const pair = root.items.find((p) => isScalar(p.key) && p.key.value === "categories");
  if (!pair) {
    const first = root.items[0];
    if (!first || !isScalar(first.key) || !first.key.range) {
      return { kind: "insert", at: root.range[0] + 1, indent: "", flow: true, newline: "", categories: [] };
    }
    const flow = root.flow === true;
    return {
      kind: "insert",
      at: first.key.range[0],
      indent: flow ? "" : indentationAt(source, first.key.range[0]),
      flow,
      newline: lineTerminator(source),
      categories: [],
    };
  }

  const key = pair.key;
  if (!isScalar(key) || typeof key.value !== "string" || key.anchor || key.tag) {
    return { kind: "unsupported", reason: UNSUPPORTED_SOURCE };
  }

  const value = pair.value;
  if (!isSeq(value) || !value.range || value.anchor || value.tag) {
    return { kind: "unsupported", reason: UNSUPPORTED_SOURCE };
  }

  const categories: string[] = [];
  for (const item of value.items) {
    if (!isScalar(item) || typeof item.value !== "string" || item.anchor || item.tag) {
      return { kind: "unsupported", reason: UNSUPPORTED_SOURCE };
    }
    categories.push(item.value);
  }

  const start = value.range[0];
  const end = value.range[1];
  const removed = source.slice(start, end);
  const newline = removed.endsWith("\r\n") ? "\r\n" : removed.endsWith("\n") ? "\n" : "";
  return { kind: "replace", categories, start, end, newline };
}

export function readCategories(source: string): CategoryRead {
  const located = locate(source);
  if (located.kind === "unsupported") return { ok: false, reason: located.reason };
  return { ok: true, categories: located.categories };
}

export function editCategories(source: string, categories: string[]): CategoryEdit {
  const located = locate(source);
  if (located.kind === "unsupported") return { ok: false, reason: located.reason };

  const json = JSON.stringify(categories);
  if (located.kind === "replace") {
    const edited =
      source.slice(0, located.start) + json + located.newline + source.slice(located.end);
    return { ok: true, source: edited };
  }

  const inserted = located.flow
    ? `categories: ${json}, `
    : `categories: ${json}${located.newline}${located.indent}`;
  const edited = source.slice(0, located.at) + inserted + source.slice(located.at);
  return { ok: true, source: edited };
}
