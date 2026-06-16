import { useQuery } from "@tanstack/react-query";
import { getJson } from "./client";
import type { TemplateSummary } from "./types";

export function useTemplates() {
  return useQuery({
    queryKey: ["templates"],
    queryFn: () => getJson<{ templates: TemplateSummary[] }>("/templates"),
  });
}
