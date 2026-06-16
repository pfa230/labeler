import { useEffect, useRef, useState } from "react";
import { NavLink, Outlet } from "react-router-dom";
import { ThemeToggle } from "./ThemeToggle";

const NAV_ITEMS = [
  { to: "/", label: "Templates" },
  { to: "/print", label: "Print" },
  { to: "/import", label: "Import" },
  { to: "/settings", label: "Settings" },
];

function navLinkClass({ isActive }: { isActive: boolean }) {
  return [
    "block rounded-md px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2",
    isActive ? "font-semibold" : "",
  ].join(" ");
}

function navLinkStyle({ isActive }: { isActive: boolean }) {
  return {
    background: isActive ? "var(--accent-soft)" : "transparent",
    color: isActive ? "var(--accent)" : "var(--ink)",
  };
}

export function Shell() {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const sidebarRef = useRef<HTMLElement>(null);
  const toggleRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!drawerOpen) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setDrawerOpen(false);
    };
    document.addEventListener("keydown", onKeyDown);
    const firstLink = sidebarRef.current?.querySelector<HTMLElement>("a, button");
    firstLink?.focus();
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      toggleRef.current?.focus();
    };
  }, [drawerOpen]);

  const closeDrawer = () => setDrawerOpen(false);

  return (
    <div className="flex h-full">
      <nav
        ref={sidebarRef}
        aria-label="Primary"
        className={[
          "fixed inset-y-0 left-0 z-40 flex w-60 flex-col border-r p-4 transition-transform md:static md:translate-x-0",
          drawerOpen ? "translate-x-0" : "-translate-x-full",
        ].join(" ")}
        style={{ background: "var(--surface)", borderColor: "var(--border)" }}
      >
        <div
          className="mb-6 px-3 text-lg font-semibold"
          style={{ color: "var(--accent)" }}
        >
          Labeler
        </div>
        <ul className="flex flex-col gap-1">
          {NAV_ITEMS.map((item) => (
            <li key={item.to}>
              <NavLink
                to={item.to}
                end={item.to === "/"}
                onClick={closeDrawer}
                className={navLinkClass}
                style={navLinkStyle}
              >
                {item.label}
              </NavLink>
            </li>
          ))}
        </ul>
        <div className="mt-auto pt-4">
          <ThemeToggle />
        </div>
      </nav>

      {drawerOpen && (
        <div
          className="fixed inset-0 z-30 md:hidden"
          style={{ background: "rgba(0,0,0,0.4)" }}
          onClick={closeDrawer}
          aria-hidden="true"
        />
      )}

      <div className="flex min-w-0 flex-1 flex-col">
        <header
          className="flex items-center gap-2 border-b p-3 md:hidden"
          style={{ borderColor: "var(--border)", background: "var(--surface)" }}
        >
          <button
            ref={toggleRef}
            type="button"
            onClick={() => setDrawerOpen((open) => !open)}
            aria-label="Toggle navigation"
            aria-expanded={drawerOpen}
            className="rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2"
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            ☰
          </button>
          <span className="font-semibold" style={{ color: "var(--accent)" }}>
            Labeler
          </span>
        </header>
        <main className="min-w-0 flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
