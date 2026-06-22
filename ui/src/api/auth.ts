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
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"] }),
  });
}
export function useSetup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: { username: string; password: string }) => sendJson("POST", "/auth/setup", c),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"] }),
  });
}
export function useLogout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => sendJson("POST", "/auth/logout", {}),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"] }),
  });
}
