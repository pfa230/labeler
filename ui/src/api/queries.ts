import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError, getJson, sendJson, del, putVoid } from "./client";
import type { TemplateListResponse, TemplateDetail, Printer, ProbeResult } from "./types";

export function useTemplates(params?: { group?: string; nested?: boolean }) {
  const queryParams = new URLSearchParams();
  if (params?.group !== undefined) queryParams.set("group", params.group);
  if (params?.nested) queryParams.set("nested", "true");
  const qs = queryParams.toString();
  const url = `/templates${qs ? `?${qs}` : ""}`;
  return useQuery({
    queryKey: ["templates", params],
    queryFn: () => getJson<TemplateListResponse>(url),
  });
}

export function useTemplateGroups() {
  return useQuery({
    queryKey: ["template-groups"],
    queryFn: () => getJson<string[]>("/template-groups"),
  });
}

export function useDeleteTemplateGroup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (groupPath: string) => {
      const encoded = groupPath.split("/").map(encodeURIComponent).join("/");
      return del(`/template-groups/${encoded}`);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["template-groups"] });
      qc.invalidateQueries({ queryKey: ["templates"] });
    },
  });
}

export function useFavorites() {
  return useQuery({ queryKey: ["favorites"], queryFn: () => getJson<string[]>("/favorites") });
}
export function useRecentTemplates() {
  return useQuery({ queryKey: ["recent-templates"], queryFn: () => getJson<string[]>("/recent-templates") });
}
export function useSetFavorite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, favorite }: { id: string; favorite: boolean }) =>
      favorite
        ? putVoid(`/favorites/${encodeURIComponent(id)}`)
        : del(`/favorites/${encodeURIComponent(id)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["favorites"] }),
  });
}
export function usePrinters() {
  return useQuery({ queryKey: ["printers"], queryFn: () => getJson<Printer[]>("/printers") });
}
export function useTemplate(id: string) {
  return useQuery({
    queryKey: ["template", id],
    queryFn: () => getJson<TemplateDetail>(`/templates/${id}`),
    enabled: !!id,
    placeholderData: keepPreviousData,
  });
}
export function useTemplateSource(id: string) {
  return useQuery({
    queryKey: ["template-source", id],
    queryFn: async () => {
      const res = await fetch(`/api/templates/${id}/source`);
      if (!res.ok) throw new Error(`source ${res.status}`);
      return res.text();
    },
    enabled: !!id,
  });
}
// Raw-YAML writes cannot go through client.ts's JSON helpers, so they build their own request — but
// they must still throw ApiError, not a bare Error.
async function yamlWrite(
  id: string,
  yaml: string,
  options?: { group?: string | null; createOnly?: boolean },
): Promise<TemplateDetail> {
  const queryParams = new URLSearchParams();
  if (options?.group !== undefined && options.group !== null) {
    queryParams.set("group", options.group);
  }
  const qs = queryParams.toString();
  const url = `/api/templates/${encodeURIComponent(id)}${qs ? `?${qs}` : ""}`;
  const headers: Record<string, string> = { "content-type": "text/yaml" };
  if (options?.createOnly) {
    headers["if-none-match"] = "*";
  }
  const res = await fetch(url, { method: "PUT", headers, body: yaml });
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new ApiError(
      res.status,
      body?.error?.code ?? "Unknown",
      body?.error?.message ?? `PUT failed (${res.status})`,
      body?.error?.details,
    );
  }
  return (await res.json()) as TemplateDetail;
}

export function useSaveTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      yaml,
      group,
      createOnly,
    }: {
      id: string;
      yaml: string;
      group?: string | null;
      createOnly?: boolean;
    }) => yamlWrite(id, yaml, { group, createOnly }),
    onSuccess: async (_data, { id, yaml }) => {
      qc.invalidateQueries({ queryKey: ["templates"] });
      qc.invalidateQueries({ queryKey: ["template-groups"] });
      qc.invalidateQueries({ queryKey: ["template", id] });
      await qc.cancelQueries({ queryKey: ["template-source", id] });
      qc.setQueryData(["template-source", id], yaml);
    },
    onError: (_err, { id }) => {
      qc.removeQueries({ queryKey: ["template-source", id] });
    },
  });
}

export function useDeleteTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/templates/${encodeURIComponent(id)}`),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ["templates"] });
      qc.invalidateQueries({ queryKey: ["template-groups"] });
      qc.invalidateQueries({ queryKey: ["favorites"] });
      qc.invalidateQueries({ queryKey: ["recent-templates"] });
      qc.removeQueries({ queryKey: ["template", id] });
      qc.removeQueries({ queryKey: ["template-source", id] });
    },
  });
}

export function useReplaceTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, yaml }: { id: string; yaml: string }) =>
      yamlWrite(id, yaml),
    onSuccess: async (_data, { id, yaml }) => {
      qc.invalidateQueries({ queryKey: ["templates"] });
      qc.invalidateQueries({ queryKey: ["template-groups"] });
      qc.invalidateQueries({ queryKey: ["template", id] });
      await qc.cancelQueries({ queryKey: ["template-source", id] });
      qc.setQueryData(["template-source", id], yaml);
    },
    onError: (_err, { id }) => {
      qc.removeQueries({ queryKey: ["template-source", id] });
    },
  });
}

export function useMoveTemplateGroup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, group }: { id: string; group: string | null }) =>
      sendJson<TemplateDetail>("PUT", `/templates/${encodeURIComponent(id)}/group`, { group }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["templates"] });
      qc.invalidateQueries({ queryKey: ["template-groups"] });
    },
  });
}

export function useVariables() {
  return useQuery({ queryKey: ["variables"], queryFn: () => getJson<Record<string, string>>("/variables") });
}

export interface ResolvedSetting {
  value: unknown; // JSON: number for retention, Record<string,string> for datetime_formats
  is_default: boolean;
}

export function useSettings() {
  return useQuery({
    queryKey: ["settings"],
    queryFn: () => getJson<Record<string, ResolvedSetting>>("/settings"),
  });
}

export function useUpdateSetting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ key, value }: { key: string; value: unknown }) =>
      sendJson<ResolvedSetting>("PUT", `/settings/${encodeURIComponent(key)}`, { value }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
}

export function previewDatetimeFormat(pattern: string) {
  return sendJson<{ sample: string }>("POST", "/datetime-formats/preview", { pattern });
}

export function useResetSetting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (key: string) => del(`/settings/${encodeURIComponent(key)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
}

export function useUpsertVariable() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ key, value }: { key: string; value: string }) =>
      sendJson<{ value: string }>("PUT", `/variables/${encodeURIComponent(key)}`, { value }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["variables"] }),
  });
}

export function useSavePrinter() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ printer, isNew }: { printer: Printer; isNew: boolean }) =>
      isNew
        ? sendJson<Printer>("POST", "/printers", printer)
        : sendJson<Printer>("PUT", `/printers/${encodeURIComponent(printer.id)}`, printer),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["printers"] }),
  });
}

export function useDeletePrinter() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/printers/${encodeURIComponent(id)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["printers"] }),
  });
}

// Test-connect an unsaved cups config; returns the printer's self-reported capabilities.
export function useProbePrinter() {
  return useMutation({
    mutationFn: (config: unknown) =>
      sendJson<ProbeResult>("POST", "/printers/probe", { kind: "cups", config }),
  });
}

export function useSetDefaultPrinter() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => sendJson("POST", `/printers/${encodeURIComponent(id)}/default`, {}),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["printers"] }),
  });
}

export function useClearDefaultPrinter() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/printers/${encodeURIComponent(id)}/default`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["printers"] }),
  });
}

export interface UserSummary {
  id: string;
  username: string;
}

export function useUsers() {
  return useQuery({ queryKey: ["users"], queryFn: () => getJson<UserSummary[]>("/users") });
}

export function useCreateUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: { username: string; password: string }) => sendJson<UserSummary>("POST", "/users", c),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["users"] }),
  });
}

export function useDeleteUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/users/${encodeURIComponent(id)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["users"] }),
  });
}

export function useChangePassword() {
  return useMutation({
    mutationFn: (c: { current_password: string; new_password: string }) =>
      sendJson<{ ok: boolean }>("POST", "/auth/password", c),
  });
}

export interface ApiToken {
  id: string;
  name: string;
  last_used_at: string | null;
  created_at: string;
}

export interface CreatedToken {
  id: string;
  name: string;
  secret: string;
}

export function useTokens() {
  return useQuery({ queryKey: ["tokens"], queryFn: () => getJson<ApiToken[]>("/tokens") });
}

export function useCreateToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: { name: string }) => sendJson<CreatedToken>("POST", "/tokens", c),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tokens"] }),
  });
}

export function useDeleteToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/tokens/${encodeURIComponent(id)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tokens"] }),
  });
}
