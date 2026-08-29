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
