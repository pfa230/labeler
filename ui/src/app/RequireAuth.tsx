import { useEffect } from "react";
import { Navigate, Outlet, useNavigate } from "react-router-dom";
import { useAuth } from "../api/auth";

export function RequireAuth() {
  const { data, isPending } = useAuth();
  const navigate = useNavigate();
  useEffect(() => {
    const handler = () => navigate("/login");
    window.addEventListener("labeler:unauthenticated", handler);
    return () => window.removeEventListener("labeler:unauthenticated", handler);
  }, [navigate]);
  if (isPending)
    return (
      <div className="p-6 text-sm" style={{ color: "var(--muted)" }}>
        Loading…
      </div>
    );
  if (!data?.authed) return <Navigate to={data?.needsSetup ? "/setup" : "/login"} replace />;
  return <Outlet />;
}
