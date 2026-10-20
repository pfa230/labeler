import { isAlias, isMap, isScalar, isSeq, parseDocument } from "yaml";

const INVALID_SOURCE = "The template source is not valid YAML.";
const UNSUPPORTED_SOURCE =
  "This template's categories cannot be edited here because they are not a direct list.";

export type CategoryRead = { ok: true; categories: string[] } | { ok: false; reason: string };
export type CategoryEdit = { ok: true; source: string } | { ok: false; reason: string };

type Located =
  | {
      kind: "replace";
      categories: string[];
      replaceFrom: number;
      end: number;
      indent: string;
      newline: string;
    }
  | { kind: "insert"; at: number; indent: string; flow: boolean; newline: string }
  | { kind: "unsupported"; reason: string };

function lineTerminator(source: string): string {
  return source.includes("\r\n") ? "\r\n" : "\n";
}

function lineStart(source: string, position: number): number {
  return source.lastIndexOf("\n", position - 1) + 1;
}

function indentationAt(source: string, position: number): string {
  const start = lineStart(source, position);
  let end = start;
  while (end < position && (source[end] === " " || source[end] === "\t")) end++;
  return source.slice(start, end);
}

function locate(source: string): Located {
  const doc = parseDocument(source, { keepSourceTokens: true });
  if (doc.errors.length > 0) return { kind: "unsupported", reason: INVALID_SOURCE };

  const root = doc.contents;
  if (!isMap(root) || !root.range || root.anchor || root.tag) {
    return { kind: "unsupported", reason: UNSUPPORTED_SOURCE };
  }

  const matches = root.items.filter((p) => {
    const key = p.key;
    if (!key) return false;
    if (isScalar(key)) return key.value === "categories";
    if (isAlias(key)) {
      const resolved = key.resolve(doc);
      return isScalar(resolved) && resolved.value === "categories";
    }
    return false;
  });
  if (matches.length === 0) {
    return absentKey(root, source);
  }
  if (matches.length > 1) {
    return { kind: "unsupported", reason: UNSUPPORTED_SOURCE };
  }

  const pair = matches[0];

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
  if (value.flow === true) {
    return { kind: "replace", categories, replaceFrom: start, end, indent: "", newline };
  }

  const existingIndent = source.slice(lineStart(source, start), start);
  const keyIndent = indentationAt(source, key.range![0]);
  const indent = existingIndent.length > keyIndent.length ? existingIndent : keyIndent + "  ";
  return { kind: "replace", categories, replaceFrom: lineStart(source, start), end, indent, newline };
}

function absentKey(root: ReturnType<typeof parseDocument>["contents"], source: string): Located {
  if (!isMap(root) || !root.range) {
    return { kind: "unsupported", reason: UNSUPPORTED_SOURCE };
  }
  if (root.flow === true) {
    const at = root.range[0] + 1;
    return { kind: "insert", at, indent: "", flow: true, newline: "" };
  }

  const first = root.items[0];
  if (!first || !isScalar(first.key) || !first.key.range) {
    const at = root.range[0] + 1;
    return { kind: "insert", at, indent: "", flow: true, newline: "" };
  }

  const explicitKey = first.srcToken?.start.find((t) => t.type === "explicit-key-ind");
  let at = lineStart(source, explicitKey?.offset ?? first.key.range[0]);
  if (at === 0 && source.charCodeAt(0) === 0xfeff) at = 1;
  let end = at;
  while (end < source.length && (source[end] === " " || source[end] === "\t")) end++;
  return {
    kind: "insert",
    at,
    indent: source.slice(at, end),
    flow: false,
    newline: lineTerminator(source),
  };
}

export function readCategories(source: string): CategoryRead {
  const located = locate(source);
  if (located.kind === "unsupported") return { ok: false, reason: located.reason };
  if (located.kind === "insert") return { ok: true, categories: [] };
  return { ok: true, categories: located.categories };
}

export function editCategories(source: string, categories: string[]): CategoryEdit {
  const located = locate(source);
  if (located.kind === "unsupported") return { ok: false, reason: located.reason };

  const json = JSON.stringify(categories);
  if (located.kind === "replace") {
    const edited =
      source.slice(0, located.replaceFrom) +
      located.indent +
      json +
      located.newline +
      source.slice(located.end);
    return { ok: true, source: edited };
  }

  if (located.flow) {
    const separator = source[located.at] === "}" ? "" : ",";
    const edited =
      source.slice(0, located.at) +
      `categories: ${json}${separator}` +
      source.slice(located.at);
    return { ok: true, source: edited };
  }

  const edited =
    source.slice(0, located.at) +
    located.indent +
    `categories: ${json}` +
    located.newline +
    source.slice(located.at);
  return { ok: true, source: edited };
}
