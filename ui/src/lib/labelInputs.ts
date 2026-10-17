import type { Param, ParamValue } from "../api/types";

// A parameter name reserves no words, so `constructor` and `__proto__` are legal ones. Reading
// `o[name]`, testing `name in o` and assigning `o[name] = v` each consult `Object.prototype` for such a
// name: the first two answer for an entry nobody holds, and the third writes no own entry at all. Every
// access keyed by a parameter name goes through these three, never through the operators.
export const hasOwnKey = (o: object, k: string): boolean => Object.prototype.hasOwnProperty.call(o, k);

export function getOwnKey<T>(o: Record<string, T>, k: string): T | undefined {
  return hasOwnKey(o, k) ? o[k] : undefined;
}

export function setOwnKey<T>(o: Record<string, T>, k: string, v: T): void {
  Object.defineProperty(o, k, { value: v, writable: true, enumerable: true, configurable: true });
}

const isBlank = (v: unknown): boolean => v === undefined || v === null || v === "";

// What a screen submits for one label (ui spec, "What a screen submits"). Parameters are walked, not
// data keys, so an absent checkbox still sends its start state and an undeclared key never leaves the
// browser. Every other blank control is omitted. A held value is sent unchanged, even one the control
// cannot show, because a grid cell keeps its value until it is edited.
export function pruneDataForSubmit(data: Record<string, unknown>, params: Param[]): Record<string, ParamValue> {
  const result: Record<string, ParamValue> = {};
  for (const param of params) {
    const v = getOwnKey(data, param.name);
    if (param.control === "list") {
      if (Array.isArray(v)) setOwnKey(result, param.name, v as string[]);
    } else if (param.control === "checkbox") {
      setOwnKey(result, param.name, isBlank(v) ? param.default === true : (v as ParamValue));
    } else if (!isBlank(v) && (typeof v === "string" || typeof v === "number" || typeof v === "boolean")) {
      setOwnKey(result, param.name, v);
    }
  }
  return result;
}

export function blankOptionLabel(param: Pick<Param, "default">): string {
  return param.default === undefined ? "" : `(default: ${String(param.default)})`;
}

// The declared min/max bound a held number violates, if any. Only a numeric control holding a number
// can violate one: a blank control, a name shadowing Object.prototype, a non-numeric string or an
// absent value is none of this rule's business. A present bound of zero counts, and the comparison is
// strict, so equality with either endpoint is accepted.
export function numericBoundError(
  param: Pick<Param, "control" | "min" | "max">,
  value: ParamValue | undefined,
): string | null {
  if (param.control !== "number" && param.control !== "integer") return null;
  if (typeof value !== "number") return null;
  if (param.min !== undefined && value < param.min) return `Must be at least ${param.min}.`;
  if (param.max !== undefined && value > param.max) return `Must be at most ${param.max}.`;
  return null;
}
