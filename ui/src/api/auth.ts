import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { sendJson } from "./client";

export interface Me {
  id: string;
  username: string;
}
export interface AuthState {
  authed: boolean;
  needsSetup: boolean;
  me?: Me;
  noAuth?: boolean;
}

export function useAuth() {
  return useQuery<AuthState>({
    queryKey: ["auth"],
    retry: false,
    queryFn: async () => {
      // /api/auth/me is auth-exempt and ALWAYS 200 with { authed, needsSetup, me? } (see Task 4).
      const res = await fetch("/api/auth/me");
      if (!res.ok) return { authed: false, needsSetup: false };
      return (await res.json()) as AuthState;
    },
  });
}

export function useLogin() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: { username: string; password: string }) => sendJson("POST", "/auth/login", c),
    // refetchType: "all" forces the ["auth"] refetch even when no observer is mounted (the page is
    // /login or /setup); the awaited mutation onSuccess then completes before the component navigates,
    // so RequireAuth sees authed:true on the first render. See #103.
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"], refetchType: "all" }),
  });
}
export function useSetup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: { username: string; password: string }) => sendJson("POST", "/auth/setup", c),
    // See useLogin: force a full refetch so navigate("/") runs against fresh auth (#103).
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"], refetchType: "all" }),
  });
}
export function useLogout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => sendJson("POST", "/auth/logout", {}),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"] }),
  });
}
