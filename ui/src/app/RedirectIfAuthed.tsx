import { Navigate, Outlet } from "react-router-dom";
import { useAuth } from "../api/auth";

// Guard for the public auth routes (/login, /setup): once auth resolves to authed, these pages must
// not stay mounted, so an authed user (or a stale redirect that lands here after auth completes) is
// sent to the app instead of getting stranded. See #103.
export function RedirectIfAuthed() {
  const { data, isPending } = useAuth();
  if (isPending)
    return (
      <div className="p-6 text-sm" style={{ color: "var(--muted)" }}>
        Loading…
      </div>
    );
  if (data?.authed) return <Navigate to="/" replace />;
  return <Outlet />;
}
