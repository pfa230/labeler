// The template catalog lives in the repo and is fetched by the BROWSER, not the server (#137):
// raw.githubusercontent.com sends `access-control-allow-origin: *`, so the fetch is allowed, and the
// YAML is then POSTed to our own API. The server never makes an outbound request, which is why an
// air-gapped deployment behaves identically — it just cannot reach the catalog, and the paste-YAML
// page still works.
//
// One constant is the whole coupling to where the catalog lives; moving it to its own repo (#138) is
// a one-line change here.
export const CATALOG_BASE = "https://raw.githubusercontent.com/pfa230/labeler/main/catalog";

export interface CatalogEntry {
  id: string;
  name: string;
  description?: string | null;
  path: string;
  category: string;
  vendor?: string | null;
  format: string;
  media_width_mm?: number | null;
  fields: string[];
}

export async function fetchCatalog(): Promise<CatalogEntry[]> {
  const res = await fetch(`${CATALOG_BASE}/index.json`);
  if (!res.ok) throw new Error(`Catalog unavailable (${res.status})`);
  return (await res.json()) as CatalogEntry[];
}

export async function fetchCatalogYaml(entry: CatalogEntry): Promise<string> {
  const res = await fetch(`${CATALOG_BASE}/${entry.path}`);
  if (!res.ok) throw new Error(`Could not download ${entry.id} (${res.status})`);
  return await res.text();
}
