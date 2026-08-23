// WCAG 2.x relative luminance and contrast ratio over the hex colours in theme.css.
//
// Only the theme test reads this today (#201). The palette asserts its own AA compliance from the
// token values parsed out of the stylesheet rather than from a copy of them here, so a later palette
// edit that drops a badge below 4.5:1 fails the suite instead of drifting from a stale duplicate.
const HEX = /^#([0-9a-f]{6})$/i;

// Throws rather than returning NaN: the caller parses these out of CSS, and a token it failed to
// find must fail loudly instead of quietly comparing nothing.
export function relativeLuminance(hex: string): number {
  const match = HEX.exec(hex.trim());
  if (!match) throw new Error(`not a 6-digit hex colour: ${hex}`);
  const [r, g, b] = [0, 2, 4]
    .map((i) => parseInt(match[1].slice(i, i + 2), 16) / 255)
    .map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const [lighter, darker] = la >= lb ? [la, lb] : [lb, la];
  return (lighter + 0.05) / (darker + 0.05);
}
