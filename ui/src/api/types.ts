export interface ApiErrorBody { error: { code: string; message: string; details?: unknown } }
export type Dimension = number | { min?: number; max?: number };
export type TemplateFormat =
  | { type: "single"; width: Dimension; height: Dimension }
  | { type: "sheet"; paper_width: number; paper_height: number; label_width: number; label_height: number; positions: [number, number][] };

export type Options = Record<string, string[]>;

export type ParamType =
  | "string"
  | "length"
  | "integer"
  | "number"
  | "boolean"
  | "enum";

export type ParamValue = string | number | boolean;

export interface ParamSpec {
  type: ParamType;
  multiline?: boolean;
  values?: string[];
  default?: ParamValue;
  min?: number;
  max?: number;
  description?: string;
}

// Layout items are tagged by `type`; only the fields the UI reads are typed.
export type LayoutItem =
  | { type: "text"; value: string; multiline?: boolean; when?: Record<string, string> }
  | { type: "qr"; value: string; when?: Record<string, string> }
  | { type: "image"; name?: string; src?: string; when?: Record<string, string> }
  | { type: "line"; when?: Record<string, string> }
  | { type: "container"; option?: Record<string, string>; when?: Record<string, string>; items: LayoutItem[] };

export interface TemplateSummary {
  id: string;
  name: string;
  description: string;
  group?: string;
  unit: string;
  dpi: number;
  format: TemplateFormat;
  params?: Record<string, ParamSpec>;
  options?: Options;
}
export interface TemplateDetail {
  id: string;
  name: string;
  description: string;
  group?: string;
  unit: string;
  dpi: number;
  format: TemplateFormat;
  params?: Record<string, ParamSpec>;
  options?: Options;
  layout: LayoutItem[];
  version?: string;
}
export interface BatchSummary { total: number; succeeded: number; failed: { index: number; error: string }[]; jobs: number }
export interface Printer { id: string; name: string; kind: string; config: unknown; is_default?: boolean }

export interface ProbeCapabilities {
  model?: string | null;
  media_width_mm?: number | null;
  resolution_dpi?: number | null;
  color: "color" | "bilevel" | "unknown";
  accepts_png: boolean;
}
export type ProbeResult =
  | { status: "ok"; capabilities: ProbeCapabilities }
  | { status: "unreachable"; detail: string };
