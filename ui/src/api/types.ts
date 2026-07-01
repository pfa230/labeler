export interface ApiErrorBody { error: { code: string; message: string; details?: unknown } }
export type Dimension = number | { min?: number; max?: number };
export type TemplateFormat =
  | { type: "single"; width: Dimension; height: Dimension }
  | { type: "sheet"; paper_width: number; paper_height: number; label_width: number; label_height: number; positions: [number, number][] };

export type Options = Record<string, string[]>;

// Layout items are tagged by `type`; only the fields the UI reads are typed.
export type LayoutItem =
  | { type: "text"; name?: string; value?: string }
  | { type: "qr"; name?: string; value?: string }
  | { type: "image"; name?: string; src?: string }
  | { type: "line" }
  | { type: "container"; option?: Record<string, string>; items: LayoutItem[] };

export interface TemplateSummary { id: string; name: string; description: string; unit: string; dpi: number; format: TemplateFormat; options?: Options }
export interface TemplateDetail {
  id: string; name: string; description: string; unit: string; dpi: number;
  format: TemplateFormat; options?: Options; layout: LayoutItem[]; version?: string;
}
export interface BatchSummary { total: number; succeeded: number; failed: { index: number; error: string }[]; jobs: number }
export interface Printer { id: string; name: string; kind: string; config: unknown; enabled: boolean; is_default?: boolean }
