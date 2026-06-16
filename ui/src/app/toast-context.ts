import { createContext, useContext } from "react";

export type ToastKind = "ok" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

export interface ToastContextValue {
  push: (toast: { kind: ToastKind; message: string }) => void;
}

export const ToastContext = createContext<ToastContextValue | null>(null);

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast must be used within a ToastProvider");
  return ctx;
}
