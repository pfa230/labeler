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

export function seedDefaultValue(param: Param): ParamValue {
  if (
    param.control === "datetime" &&
    typeof param.default === "string" &&
    /^\d{4}-\d{2}-\d{2}$/.test(param.default)
  ) {
    return `${param.default}T00:00`;
  }
  return param.default!;
}

// A checkbox always sends its value: one whose key is absent or blank sends its start state, which is
// its published default (a JSON boolean) or false. A held value is sent unchanged, even one the
// control cannot show, because a grid cell keeps its value until it is edited.
export function pruneDataForSubmit(
  data: Record<string, unknown>,
  params: Param[],
  deferred?: Record<string, boolean>,
): Record<string, ParamValue> {
  const result: Record<string, ParamValue> = {};
  const byName = new Map(params.map((p) => [p.name, p]));
  for (const [k, v] of Object.entries(data)) {
    if (deferred && getOwnKey(deferred, k)) continue;
    const param = byName.get(k);
    if (!param) continue;
    if (param.control === "list") {
      if (Array.isArray(v)) {
        setOwnKey(result, k, v as ParamValue);
      }
      continue;
    }
    if (v === "" && param.control !== "text" && param.control !== "textarea" && param.control !== "image") continue;
    if (typeof v === "string" || typeof v === "number" || typeof v === "boolean") {
      setOwnKey(result, k, v as ParamValue);
    }
  }
  for (const param of params) {
    if (param.control === "checkbox" && !hasOwnKey(result, param.name)) {
      setOwnKey(result, param.name, param.default === true);
    }
  }
  return result;
}
