import { useJobs } from "../api/queries";
import type { JobRecord } from "../api/types";

function actorLabel(job: JobRecord): string {
  if (job.actor_username) return job.actor_username;
  return job.actor_id === "" ? "unknown" : job.actor_id;
}

function timeLabel(timestamp: string): string {
  return new Date(timestamp).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
    timeZoneName: "short",
  });
}

export function PrintHistory() {
  const { data, isPending, isError, error, refetch, isFetching } = useJobs();

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Print history</h1>
      {isPending ? (
        <p style={{ color: "var(--muted)" }}>Loading print history…</p>
      ) : isError ? (
        <div className="flex flex-col items-start gap-2" role="alert">
          <p style={{ color: "var(--bad)" }}>
            {error instanceof Error ? error.message : "Couldn't load print history."}
          </p>
          <button
            type="button"
            onClick={() => refetch()}
            disabled={isFetching}
            className="rounded-md border px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2"
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Retry
          </button>
        </div>
      ) : data.length === 0 ? (
        <p style={{ color: "var(--muted)" }}>No print jobs recorded yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr style={{ color: "var(--muted)" }}>
                <th className="py-2 pr-4 font-medium">Time</th>
                <th className="py-2 pr-4 font-medium">Template</th>
                <th className="py-2 pr-4 font-medium">Printer</th>
                <th className="py-2 pr-4 font-medium">Actor</th>
                <th className="py-2 font-medium">Outcome</th>
              </tr>
            </thead>
            <tbody>
              {data.map((job) => (
                <tr key={job.id} className="border-t" style={{ borderColor: "var(--border)" }}>
                  <td className="py-2 pr-4 align-top">
                    <time dateTime={job.timestamp}>{timeLabel(job.timestamp)}</time>
                  </td>
                  <td className="py-2 pr-4 align-top">
                    <code className="rounded px-1.5 py-0.5 text-xs" style={{ background: "var(--paper)" }}>
                      {job.template_id}
                    </code>
                  </td>
                  <td className="py-2 pr-4 align-top">{job.printer_id ?? "—"}</td>
                  <td className="py-2 pr-4 align-top">{actorLabel(job)}</td>
                  <td className="py-2 align-top">
                    <span style={{ color: job.status === "ok" ? "var(--good)" : "var(--bad)" }}>
                      {job.status === "ok" ? "Sent to printer" : "Failed"}
                    </span>
                    {job.status === "failed" && job.error && (
                      <details className="mt-1">
                        <summary className="cursor-pointer" style={{ color: "var(--ink)" }}>
                          Error detail
                        </summary>
                        <pre className="mt-1 whitespace-pre-wrap break-words text-xs" style={{ color: "var(--bad)" }}>
                          {job.error}
                        </pre>
                      </details>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
