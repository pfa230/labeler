import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson, sendJson, del } from "./client";

export interface FieldTransform {
  resource: string;
  source: string;
  pattern: string;
}

export interface Connection {
  id: string;
  connector: string;
  name: string;
  base_url: string;
  public_url?: string | null;
  enabled: boolean;
  has_credential: boolean;
  transforms: FieldTransform[];
}
export interface ConnectionInput {
  connector: string;
  name: string;
  base_url: string;
  public_url?: string | null;
  credential?: string;
  enabled?: boolean;
  transforms?: FieldTransform[];
}

export type ConnectorView = "table" | "tree";
export type FieldType = "text" | "number" | "money" | "date" | "badge";
export type FilterType = "search" | "location_id" | "label_id";
export type Tier = "cheap" | "hydrated" | "derived";

export interface FieldSpec { key: string; label: string; ty: FieldType; tier: Tier; multi_valued: boolean; transform_source: boolean }
export interface FilterSpec { key: string; label: string; ty: FilterType }
export interface ResourceSpec {
  id: string;
  label: string;
  view: ConnectorView;
  columns: FieldSpec[];
  filters: FilterSpec[];
  dynamic_source_prefix: string | null;
  fields_incomplete: boolean;
}
export interface RelationshipSpec { id: string; label: string; from: string; to: string }
export interface ConnectorSchema { version: string; resources: ResourceSpec[]; relationships: RelationshipSpec[] }

export interface RowRef { resource: string; key: string }
export interface SelectedRow { resource: string; key: string; label: string; breadcrumb?: string; lastSeen: number }
export type CellValue = string | number | string[]; // backend untagged Text|Number|List
export interface DisplayRow { id: RowRef; cells: Record<string, CellValue>; url?: string }
export type FilterValue = string | string[];
export interface BrowseParent { relationship: string; key: string }
export interface BrowseRequest {
  resource: string;
  filters?: Record<string, FilterValue>;
  parent?: BrowseParent;
  cursor?: string;
  page_size?: number;
}
export interface BrowsePage { rows: DisplayRow[]; next_cursor: string | null; has_more: boolean; count: number | null }

export interface MaterializeRequest { rows: RowRef[]; fields: string[]; expansion: "as_listed" }
export interface LabelRowResult { source: RowRef; data: Record<string, string | string[]> }

export function useConnections() {
  return useQuery({ queryKey: ["connections"], queryFn: () => getJson<Connection[]>("/connections") });
}

export function useSaveConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: ["connection"],
    mutationFn: ({ input, id }: { input: ConnectionInput; id?: string }) =>
      id === undefined
        ? sendJson<Connection>("POST", "/connections", input)
        : sendJson<Connection>("PUT", `/connections/${encodeURIComponent(id)}`, input),
    onSuccess: (_data, variables) => {
      qc.removeQueries({ queryKey: ["connections"] });
      if (variables.id !== undefined) {
        qc.removeQueries({ queryKey: ["connector-schema", variables.id] });
      }
    },
  });
}

export function useDeleteConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: ["connection"],
    mutationFn: (id: string) => del(`/connections/${encodeURIComponent(id)}`),
    onSuccess: (_data, id) => {
      qc.removeQueries({ queryKey: ["connections"] });
      qc.removeQueries({ queryKey: ["connector-schema", id] });
      qc.removeQueries({ queryKey: ["settings"] });
    },
  });
}

export function useSetDefaultConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: ["connection"],
    mutationFn: (id: string) =>
      sendJson<{ value: unknown; is_default: boolean }>("PUT", "/settings/default_connection_id", { value: id }),
    onSuccess: () => {
      qc.removeQueries({ queryKey: ["settings"] });
    },
  });
}

export function useClearDefaultConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: ["connection"],
    mutationFn: () => del("/settings/default_connection_id"),
    onSuccess: () => {
      qc.removeQueries({ queryKey: ["settings"] });
    },
  });
}

export function useConnectorSchema(id: string) {
  return useQuery({
    queryKey: ["connector-schema", id],
    queryFn: () => getJson<ConnectorSchema>(`/connections/${encodeURIComponent(id)}/schema`),
    enabled: !!id,
  });
}

export function browseConnection(id: string, req: BrowseRequest): Promise<BrowsePage> {
  return sendJson<BrowsePage>("POST", `/connections/${encodeURIComponent(id)}/browse`, req);
}

export function materializeConnection(id: string, req: MaterializeRequest): Promise<LabelRowResult[]> {
  return sendJson<LabelRowResult[]>("POST", `/connections/${encodeURIComponent(id)}/materialize`, req);
}

export interface TransformPreviewRequest {
  transforms: FieldTransform[];
  rule: number;
  page_size?: number;
}

export interface TransformPreviewRow {
  id: RowRef;
  source_value?: string;
  matched: boolean;
  value_truncated: boolean;
  derived?: Record<string, string>;
}

export interface TransformPreviewResponse {
  rule: number;
  resource: string;
  source: string;
  row_count: number;
  matched_count: number;
  rows: TransformPreviewRow[];
}

export function previewTransforms(id: string, req: TransformPreviewRequest): Promise<TransformPreviewResponse> {
  return sendJson<TransformPreviewResponse>("POST", `/connections/${encodeURIComponent(id)}/transforms/preview`, req);
}

