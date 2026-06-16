export interface ApiErrorBody { error: { code: string; message: string; details?: unknown } }
export interface TemplateSummary { id: string; name: string; description: string; unit: string; dpi: number; format: { type: string } }
export interface BatchSummary { total: number; succeeded: number; failed: { index: number; error: string }[]; jobs: number }
