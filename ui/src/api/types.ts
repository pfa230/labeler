export interface ApiErrorBody { error: { code: string; message: string; details?: unknown } }
export type Dimension = number | { min?: number; max?: number };
export type TemplateFormat =
  | { type: "single"; width: Dimension; height: Dimension }
  | { type: "sheet"; paper_width: number; paper_height: number; label_width: number; label_height: number; positions: [number, number][] };

export type ParamValue = string | number | boolean;

export interface ParamDefaultError {
  reason: string;
  message: string;
  token?: string;
  value?: string;
}

export type ParamDefaultReport =
  | { resolved: ParamValue }
  | { error: ParamDefaultError };

export interface ParamSpec {
  type: "string" | "number" | "integer" | "boolean" | "enum" | "length" | "datetime";
  default?: ParamValue;
  values?: string[];
  min?: number;
  max?: number;
  multiline?: boolean;
  time?: boolean;
  description?: string;
}

export type InputControl =
  | "text"
  | "textarea"
  | "select"
  | "checkbox"
  | "number"
  | "integer"
  | "image"
  | "date"
  | "datetime";

export interface InputSpec {
  name: string;
  control: InputControl;
  slider?: boolean;
  required?: boolean;
  default?: ParamValue;
  default_error?: ParamDefaultError;
  values?: string[];
  min?: number;
  max?: number;
  unit?: string;
  description?: string;
  interpolated?: boolean;
  truncated_elsewhere?: boolean;
}

export interface TemplateInputs {
  default: InputSpec[];
  all: InputSpec[];
}

export interface BrokenTemplate {
  path: string;
  reason: string;
  error?: string;
}

export interface TemplateSummary {
  id: string;
  name: string;
  description: string;
  group?: string;
  unit: string;
  dpi: number;
  format: TemplateFormat;
  params?: Record<string, ParamSpec>;
}

export interface TemplateListResponse {
  templates: TemplateSummary[];
  broken?: BrokenTemplate[];
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
  param_defaults?: Record<string, ParamDefaultReport>;
  inputs: TemplateInputs;
  variables: string[];
  version?: string;
}

export interface TemplateInputsRequest {
  labels: { data?: Record<string, unknown>; option?: Record<string, string> }[];
}

export interface TemplateInputsResponse {
  inputs: InputSpec[][];
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
