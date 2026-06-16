import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson } from "./client"; // only getJson is used here; do NOT import sendJson (noUnusedLocals)
import type { TemplateSummary, TemplateDetail, Printer } from "./types";

export function useTemplates() {
  return useQuery({ queryKey: ["templates"], queryFn: () => getJson<{ templates: TemplateSummary[] }>("/templates") });
}
export function usePrinters() {
  return useQuery({ queryKey: ["printers"], queryFn: () => getJson<Printer[]>("/printers") });
}
export function useTemplate(id: string) {
  return useQuery({ queryKey: ["template", id], queryFn: () => getJson<TemplateDetail>(`/templates/${id}`), enabled: !!id });
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
