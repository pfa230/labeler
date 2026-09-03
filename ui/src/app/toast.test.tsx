import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { ToastProvider } from "./toast";
import { useToast } from "./toast-context";

const DEDUPE_WINDOW_MS = 4000;
const DISMISS_AFTER_MS = 5000;

describe("ToastProvider", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("clears pending dismissal timers on unmount so no timers outlive the provider", () => {
    vi.useFakeTimers();
    const clearTimeoutSpy = vi.spyOn(globalThis, "clearTimeout");
    let pushFn!: (toast: { kind: "ok" | "error"; message: string }) => void;

    function Consumer() {
      const { push } = useToast();
      pushFn = push;
      return <div>consumer</div>;
    }

    const { unmount } = render(
      <ToastProvider>
        <Consumer />
      </ToastProvider>,
    );

    act(() => {
      pushFn({ kind: "ok", message: "test-unmount" });
    });

    expect(screen.getByText("test-unmount")).toBeInTheDocument();
    expect(vi.getTimerCount()).toBe(1);

    clearTimeoutSpy.mockClear();
    unmount();

    // The provider must clear the pending timer on unmount
    expect(clearTimeoutSpy).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);

    // Advancing timers past DISMISS_AFTER_MS should have no timers to fire
    act(() => {
      vi.advanceTimersByTime(DISMISS_AFTER_MS + 1000);
    });
  });

  it("clears the timer when manually dismissed before DISMISS_AFTER_MS", () => {
    vi.useFakeTimers();
    const clearTimeoutSpy = vi.spyOn(globalThis, "clearTimeout");
    let pushFn!: (toast: { kind: "ok" | "error"; message: string }) => void;

    function Consumer() {
      const { push } = useToast();
      pushFn = push;
      return <div>consumer</div>;
    }

    render(
      <ToastProvider>
        <Consumer />
      </ToastProvider>,
    );

    act(() => {
      pushFn({ kind: "ok", message: "manual-dismiss" });
    });

    const button = screen.getByRole("button", { name: "manual-dismiss" });
    expect(button).toBeInTheDocument();
    expect(vi.getTimerCount()).toBe(1);

    clearTimeoutSpy.mockClear();
    fireEvent.click(button);

    // Toast is dismissed from DOM
    expect(screen.queryByText("manual-dismiss")).not.toBeInTheDocument();
    // In fixed code, the pending timer handle must be cancelled immediately
    expect(clearTimeoutSpy).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("drops a fired timer handle so unmount only clears remaining active timers", () => {
    vi.useFakeTimers();
    const clearTimeoutSpy = vi.spyOn(globalThis, "clearTimeout");
    let pushFn!: (toast: { kind: "ok" | "error"; message: string }) => void;

    function Consumer() {
      const { push } = useToast();
      pushFn = push;
      return <div>consumer</div>;
    }

    const { unmount } = render(
      <ToastProvider>
        <Consumer />
      </ToastProvider>,
    );

    // 1. Toast 1 is pushed at t=0
    act(() => {
      pushFn({ kind: "ok", message: "fired-toast" });
    });
    expect(vi.getTimerCount()).toBe(1);

    // 2. Advance to trigger auto-dismissal of Toast 1
    act(() => {
      vi.advanceTimersByTime(DISMISS_AFTER_MS);
    });
    expect(screen.queryByText("fired-toast")).not.toBeInTheDocument();
    expect(vi.getTimerCount()).toBe(0);

    // 3. Toast 2 is pushed at t=5000 (still active when unmount occurs)
    act(() => {
      pushFn({ kind: "ok", message: "active-toast" });
    });
    expect(vi.getTimerCount()).toBe(1);

    clearTimeoutSpy.mockClear();
    unmount();

    // Only Toast 2 (active) should be cleared on unmount; Toast 1 (fired) must have been dropped
    expect(clearTimeoutSpy).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("never holds an entry in recent older than DEDUPE_WINDOW_MS", () => {
    vi.useFakeTimers();
    const deleteSpy = vi.spyOn(Map.prototype, "delete");

    let pushFn!: (toast: { kind: "ok" | "error"; message: string }) => void;

    function Consumer() {
      const { push } = useToast();
      pushFn = push;
      return <div>consumer</div>;
    }

    render(
      <ToastProvider>
        <Consumer />
      </ToastProvider>,
    );

    const deletedToastKeys = () =>
      deleteSpy.mock.calls
        .map(([k]) => k)
        .filter((k) => typeof k === "string" && (k.startsWith("ok:") || k.startsWith("error:")));

    // 1. Raise a toast at t = 0
    act(() => {
      pushFn({ kind: "ok", message: "first" });
    });

    expect(screen.getByText("first")).toBeInTheDocument();
    expect(deletedToastKeys()).toEqual([]);

    // 2. Advance past DEDUPE_WINDOW_MS
    act(() => {
      vi.advanceTimersByTime(DEDUPE_WINDOW_MS + 500);
    });

    // 3. Raise a different toast
    act(() => {
      pushFn({ kind: "ok", message: "second" });
    });

    expect(screen.getByText("second")).toBeInTheDocument();
    // 4. Observe the first key was pruned when the second toast was pushed
    expect(deletedToastKeys()).toContain("ok:first");
  });

  it("prunes multiple expired entries when pushing a new toast", () => {
    vi.useFakeTimers();
    const deleteSpy = vi.spyOn(Map.prototype, "delete");

    let pushFn!: (toast: { kind: "ok" | "error"; message: string }) => void;

    function Consumer() {
      const { push } = useToast();
      pushFn = push;
      return <div>consumer</div>;
    }

    render(
      <ToastProvider>
        <Consumer />
      </ToastProvider>,
    );

    act(() => {
      pushFn({ kind: "ok", message: "msg-1" });
    });
    act(() => {
      vi.advanceTimersByTime(1000);
      pushFn({ kind: "ok", message: "msg-2" });
    });
    act(() => {
      vi.advanceTimersByTime(1000);
      pushFn({ kind: "ok", message: "msg-3" });
    });

    deleteSpy.mockClear();

    // Advance so msg-1 and msg-2 are expired (> 4000ms), but msg-3 is not yet
    act(() => {
      vi.advanceTimersByTime(3500); // now t = 5500
      pushFn({ kind: "ok", message: "msg-4" });
    });

    const deletedToastKeys = deleteSpy.mock.calls
      .map(([k]) => k)
      .filter((k) => typeof k === "string" && (k.startsWith("ok:") || k.startsWith("error:")));

    // msg-1 and msg-2 must be pruned; msg-3 and msg-4 must remain
    expect(deletedToastKeys).toEqual(["ok:msg-1", "ok:msg-2"]);
  });

  it("suppresses duplicate toast within DEDUPE_WINDOW_MS", () => {
    vi.useFakeTimers();
    let pushFn!: (toast: { kind: "ok" | "error"; message: string }) => void;

    function Consumer() {
      const { push } = useToast();
      pushFn = push;
      return <div>consumer</div>;
    }

    render(
      <ToastProvider>
        <Consumer />
      </ToastProvider>,
    );

    act(() => {
      pushFn({ kind: "ok", message: "duplicate" });
    });

    expect(screen.getAllByText("duplicate")).toHaveLength(1);

    // Advance 2 seconds (within 4s window)
    act(() => {
      vi.advanceTimersByTime(2000);
    });

    act(() => {
      pushFn({ kind: "ok", message: "duplicate" });
    });

    // Still only 1 toast rendered
    expect(screen.getAllByText("duplicate")).toHaveLength(1);
  });

  it("admits repeat toast after DEDUPE_WINDOW_MS has elapsed", () => {
    vi.useFakeTimers();
    let pushFn!: (toast: { kind: "ok" | "error"; message: string }) => void;

    function Consumer() {
      const { push } = useToast();
      pushFn = push;
      return <div>consumer</div>;
    }

    render(
      <ToastProvider>
        <Consumer />
      </ToastProvider>,
    );

    act(() => {
      pushFn({ kind: "ok", message: "repeat" });
    });

    expect(screen.getAllByText("repeat")).toHaveLength(1);

    // Advance past DEDUPE_WINDOW_MS (4000ms)
    act(() => {
      vi.advanceTimersByTime(DEDUPE_WINDOW_MS + 100);
    });

    act(() => {
      pushFn({ kind: "ok", message: "repeat" });
    });

    // Both should now be rendered (or 2 items present)
    expect(screen.getAllByText("repeat")).toHaveLength(2);
  });

  it("renders ok and error toasts with correct style and role", () => {
    let pushFn!: (toast: { kind: "ok" | "error"; message: string }) => void;

    function Consumer() {
      const { push } = useToast();
      pushFn = push;
      return <div>consumer</div>;
    }

    render(
      <ToastProvider>
        <Consumer />
      </ToastProvider>,
    );

    act(() => {
      pushFn({ kind: "ok", message: "success message" });
      pushFn({ kind: "error", message: "error message" });
    });

    const okBtn = screen.getByRole("button", { name: "success message" });
    const errBtn = screen.getByRole("button", { name: "error message" });

    expect(okBtn.style.borderColor).toBe("var(--good)");
    expect(errBtn.style.borderColor).toBe("var(--bad)");
  });
});
