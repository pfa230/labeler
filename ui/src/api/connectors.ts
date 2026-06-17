import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson, sendJson, del } from "./client";

export interface Connection {
  id: string;
  connector: string;
  name: string;
  base_url: string;
  enabled: boolean;
  has_credential: boolean;
}
export interface ConnectionInput {
  connector: string;
  name: string;
  base_url: string;
  credential?: string;
  enabled?: boolean;
}

export type ConnectorView = "table" | "tree";
export type FieldType = "text" | "number" | "money" | "date" | "badge";
export type FilterType = "search" | "location_id" | "label_id";
export type Tier = "cheap" | "hydrated" | "derived";

export interface FieldSpec { key: string; label: string; ty: FieldType; tier: Tier }
export interface FilterSpec { key: string; label: string; ty: FilterType }
export interface ResourceSpec { id: string; label: string; view: ConnectorView; columns: FieldSpec[]; filters: FilterSpec[] }
export interface RelationshipSpec { id: string; label: string; from: string; to: string }
export interface ConnectorSchema { version: string; resources: ResourceSpec[]; relationships: RelationshipSpec[] }

export interface RowRef { resource: string; key: string }
export type CellValue = string | number; // backend untagged Text|Number
export interface DisplayRow { id: RowRef; cells: Record<string, CellValue> }
export interface BrowseParent { relationship: string; key: string }
export interface BrowseRequest {
  resource: string;
  filters?: Record<string, string>;
  parent?: BrowseParent;
  cursor?: string;
  page_size?: number;
}
export interface BrowsePage { rows: DisplayRow[]; next_cursor: string | null; has_more: boolean; count: number | null }

export interface MaterializeRequest { rows: RowRef[]; fields: string[]; expansion: "as_listed" }
export interface LabelRowResult { source: RowRef; data: Record<string, string> }

export function useConnections() {
  return useQuery({ queryKey: ["connections"], queryFn: () => getJson<Connection[]>("/connections") });
}

export function useSaveConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ input, id }: { input: ConnectionInput; id?: string }) =>
      id === undefined
        ? sendJson<Connection>("POST", "/connections", input)
        : sendJson<Connection>("PUT", `/connections/${encodeURIComponent(id)}`, input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["connections"] }),
  });
}

export function useDeleteConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/connections/${encodeURIComponent(id)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["connections"] }),
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
