import { isMap, isNode, parseDocument } from "yaml";

export function cloneTemplateSource(source: string, copyNumber: number): string {
  const doc = parseDocument(source);
  if (doc.errors.length > 0) throw doc.errors[0];
  const name = isMap(doc.contents) ? doc.contents.get("name", true) : undefined;
  if (!isNode(name) || !name.range) throw new Error("The template source has no name value.");

  const [start, end] = name.range;
  const removed = source.slice(start, end);
  // Block scalars include their final line terminator in the value range.
  const newline = removed.endsWith("\r\n") ? "\r\n" : removed.endsWith("\n") ? "\n" : "";
  const suffix = copyNumber === 1 ? "copy" : `copy ${copyNumber}`;
  return source.slice(0, start) + JSON.stringify(`${doc.toJS().name} (${suffix})`) + newline + source.slice(end);
}
