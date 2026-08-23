import type { TemplateFormat } from "../api/types";

// #201: single and sheet were two identical accent pills, so the one attribute that decides how a
// template prints was the one the eye could not pick up without reading. Three cues separate them
// now, each sufficient on its own: the icon, the colour, and the text, since a sheet states its
// position count.
//
// The border, not the fill, is what delineates the chip. A selected card is tinted --accent-soft
// (pages/Templates.tsx:63), which is exactly what the single chip is filled with, so a fill-only
// chip vanishes the moment a card is selected. See ADR-0066.
const TOKENS = {
  single: { fg: "var(--accent-deep)", fill: "var(--accent-soft)" },
  sheet: { fg: "var(--info)", fill: "var(--info-soft)" },
} as const;

// Two columns by three rows, occupying 8 x 12 of the viewBox: portrait like a sheet of stock,
// against the single icon's landscape 12 x 6. Three columns in the same width would leave cells
// under 1.7 of the 12 pixels this renders at, where antialiasing greys the pattern into a smear.
const SHEET_CELLS = [0, 4.5, 9].flatMap((y) => [2, 7].map((x) => ({ x, y })));

function FormatIcon({ type }: { type: TemplateFormat["type"] }) {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      width="12"
      height="12"
      viewBox="0 0 12 12"
      fill="currentColor"
    >
      {type === "single" ? (
        <rect x="0" y="3" width="12" height="6" rx="1" />
      ) : (
        SHEET_CELLS.map(({ x, y }) => (
          <rect key={`${x}-${y}`} x={x} y={y} width="3" height="3" />
        ))
      )}
    </svg>
  );
}

export function FormatBadge({ format }: { format: TemplateFormat }) {
  const { fg, fill } = TOKENS[format.type];
  return (
    <span
      data-format={format.type}
      className="inline-flex shrink-0 items-center gap-1 whitespace-nowrap rounded-full border px-2 py-0.5 text-xs font-medium"
      style={{ background: fill, color: fg, borderColor: fg }}
    >
      <FormatIcon type={format.type} />
      {/* One text node, so the whole label is one getByText target and what a screen reader
          conveys is exactly the string a sighted user reads. */}
      <span>{format.type === "sheet" ? `sheet · ${format.positions.length}` : "single"}</span>
    </span>
  );
}
