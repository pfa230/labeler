import { useEffect, useState } from "react";

/** Live media-query match. Returns false where matchMedia is unavailable (non-browser). */
export function useMediaQuery(query: string): boolean {
  const get = () =>
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia(query).matches
      : false;
  const [matches, setMatches] = useState(get);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const mql = window.matchMedia(query);
    const onChange = (e: MediaQueryListEvent | { matches: boolean }) => setMatches(e.matches);
    setMatches(mql.matches);
    mql.addEventListener("change", onChange as EventListener);
    return () => mql.removeEventListener("change", onChange as EventListener);
  }, [query]);

  return matches;
}
