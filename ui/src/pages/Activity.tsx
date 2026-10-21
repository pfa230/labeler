import { useState } from "react";
import { useActivity } from "../api/queries";

const actionLabels = {
  "template.create": "Create template",
  "template.replace": "Replace template",
  "template.delete": "Delete template",
  "template.reload": "Reload registry",
};

export function Activity() {
  const [beforeId, setBeforeId] = useState<number | null>(null);
  const { data, isPending, isError, error } = useActivity(beforeId);

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Activity</h1>
      {isPending ? (
        <p style={{ color: "var(--muted)" }}>Loading activity…</p>
      ) : isError ? (
        <p role="alert" style={{ color: "var(--bad)" }}>{error.message}</p>
      ) : data.entries.length === 0 ? (
        <p style={{ color: "var(--muted)" }}>No activity recorded yet.</p>
      ) : (
        <div className="overflow-x-auto rounded-md border p-4" style={{ borderColor: "var(--border)", background: "var(--surface)" }}>
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b" style={{ borderColor: "var(--border)", color: "var(--muted)" }}>
                {["Time", "Actor", "Action", "Target", "Outcome"].map((heading) => (
                  <th key={heading} scope="col" className="pb-2 pr-4 font-medium">{heading}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.entries.map((entry) => (
                <tr key={entry.id} className="border-b last:border-0" style={{ borderColor: "var(--border)" }}>
                  <td className="py-2 pr-4 align-top whitespace-nowrap">
                    <time dateTime={entry.time}>{new Date(entry.time).toLocaleString()}</time>
                  </td>
                  <td className="py-2 pr-4 align-top">{entry.actor_name}</td>
                  <td className="py-2 pr-4 align-top">{actionLabels[entry.action]}</td>
                  <td className="py-2 pr-4 align-top"><code>{entry.target}</code></td>
                  <td className="py-2 align-top">
                    <span style={{ color: entry.outcome === "success" ? "var(--good)" : "var(--bad)" }}>
                      {entry.outcome === "success" ? "Success" : "Failure"}
                    </span>
                    {entry.error_code && <div><code>{entry.error_code}</code></div>}
                    {entry.error_reason && <div><code>{entry.error_reason}</code></div>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="flex gap-2">
        {beforeId !== null && (
          <button type="button" onClick={() => setBeforeId(null)} className="rounded-md border px-3 py-2 text-sm" style={{ borderColor: "var(--border)" }}>
            Newest
          </button>
        )}
        {!isError && data?.next_before_id != null && (
          <button type="button" onClick={() => setBeforeId(data.next_before_id)} className="rounded-md border px-3 py-2 text-sm" style={{ borderColor: "var(--border)" }}>
            Older
          </button>
        )}
      </div>
    </div>
  );
}
