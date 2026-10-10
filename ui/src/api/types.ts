export interface ApiErrorBody { error: { code: string; message: string; details?: unknown } }
export type Dimension = number | { min?: number; max?: number };
export type TemplateFormat =
  | { type: "single"; width: Dimension; height: Dimension }
  | { type: "sheet"; paper_width: number; paper_height: number; label_width: number; label_height: number; positions: [number, number][] };

export type ParamValue = string | number | boolean | string[];

export type ParamControl =
  | "text"
  | "textarea"
  | "select"
  | "checkbox"
  | "number"
  | "integer"
  | "image"
  | "date"
  | "datetime"
  | "list";

export interface Param {
  name: string;
  type: "string" | "number" | "integer" | "boolean" | "enum" | "length" | "datetime" | "list";
  control: ParamControl;
  default?: ParamValue;
  description?: string;
  values?: string[];
  min?: number;
  max?: number;
  multiline?: boolean;
  time?: boolean;
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
  categories: string[];
  unit: string;
  dpi: number;
  format: TemplateFormat;
  params: Param[];
}

export interface TemplateListResponse {
  templates: TemplateSummary[];
  broken?: BrokenTemplate[];
}

export interface TemplateDetail {
  id: string;
  name: string;
  description: string;
  categories: string[];
  unit: string;
  dpi: number;
  format: TemplateFormat;
  params: Param[];
  variables: string[];
}

export interface PrintSummary { total: number; sent: number; failed: { index: number; error: string }[]; jobs: number }
export interface RenderProfile { color_mode?: "color" | "bilevel"; resolution?: number }

export interface Printer {
  id: string;
  name: string;
  uri: string;
  username?: string;
  ca_cert?: string;
  insecure: boolean;
  render?: RenderProfile;
}

// The probe body: a printer's connection fields without its id or name.
export interface PrinterConnection {
  uri: string;
  username?: string;
  ca_cert?: string;
  insecure?: boolean;
  render?: RenderProfile;
}

// The PUT body; POST adds the id.
export interface PrinterUpdate extends PrinterConnection { name: string }

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
