import { useState } from "react";

// The server's thumbnail for a template, or a placeholder once it fails to load. A failure belongs to
// one template, so a caller that reuses the component across templates keys it by id. `version` changes
// the URL when the template changes under an open page, because a browser serves a repeated image URL
// from memory without asking the server.
export function TemplateThumbnail({ id, name, version }: { id: string; name: string; version?: number }) {
  const [failed, setFailed] = useState(false);
  if (failed) {
    return (
      <div
        className="flex aspect-[3/1] items-center justify-center rounded-md border text-xs"
        style={{ background: "var(--paper)", borderColor: "var(--border)", color: "var(--muted)" }}
        aria-hidden="true"
      >
        preview
      </div>
    );
  }
  return (
    <img
      src={`/api/templates/${id}/thumbnail${version === undefined ? "" : `?v=${version}`}`}
      alt={`${name} preview`}
      loading="lazy"
      onError={() => setFailed(true)}
      className="aspect-[3/1] w-full rounded-md border object-contain"
      style={{ background: "var(--paper)", borderColor: "var(--border)" }}
    />
  );
}
