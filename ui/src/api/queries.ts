import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson, sendJson, del } from "./client";
import type { TemplateSummary, TemplateDetail, Printer } from "./types";

export function useTemplates() {
  return useQuery({ queryKey: ["templates"], queryFn: () => getJson<{ templates: TemplateSummary[] }>("/templates") });
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
export function useCreateTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (yaml: string) => {
      const res = await fetch("/api/templates", { method: "POST", headers: { "content-type": "text/yaml" }, body: yaml });
      if (!res.ok) {
        const body = await res.json().catch(() => null);
        throw new Error(body?.error?.message ?? `create failed (${res.status})`);
      }
      return (await res.json()) as TemplateDetail;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}

export function useVariables() {
  return useQuery({ queryKey: ["variables"], queryFn: () => getJson<Record<string, string>>("/variables") });
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
