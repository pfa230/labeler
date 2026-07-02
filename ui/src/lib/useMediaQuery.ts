import { useCallback, useSyncExternalStore } from "react";

const canMatch = () => typeof window !== "undefined" && typeof window.matchMedia === "function";

/** Live media-query match via useSyncExternalStore. Returns false where matchMedia is unavailable. */
export function useMediaQuery(query: string): boolean {
  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      if (!canMatch()) return () => {};
      const mql = window.matchMedia(query);
      mql.addEventListener("change", onStoreChange);
      return () => mql.removeEventListener("change", onStoreChange);
    },
    [query],
  );
  return useSyncExternalStore(subscribe, () => (canMatch() ? window.matchMedia(query).matches : false));
}
