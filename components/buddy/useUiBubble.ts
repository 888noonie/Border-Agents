import { useCallback, useEffect, useRef, useState } from "react";

export type UiBubblePhase = "" | "entering" | "visible" | "leaving";

const ENTER_MS = 420;
const EXIT_MS = 180;
const AUTO_HIDE_MIN_MS = 2200;
const AUTO_HIDE_MAX_MS = 12000;
const AUTO_HIDE_BASE_MS = 1400;

function autoHideDelay(text: string) {
  const trimmed = text.trim();
  if (!trimmed) {
    return 0;
  }

  const words = trimmed.split(/\s+/).length;
  const chars = trimmed.length;
  const estimate = AUTO_HIDE_BASE_MS + chars * 34 + words * 120;
  return Math.min(AUTO_HIDE_MAX_MS, Math.max(AUTO_HIDE_MIN_MS, estimate));
}

export function useUiBubble(text: string, enabled = true, persist = false) {
  const [mounted, setMounted] = useState(false);
  const [phase, setPhase] = useState<UiBubblePhase>("");
  const [displayText, setDisplayText] = useState("");
  const enterTimerRef = useRef(0);
  const exitTimerRef = useRef(0);
  const hideTimerRef = useRef(0);
  const generationRef = useRef(0);

  const clearTimers = useCallback(() => {
    if (enterTimerRef.current) {
      window.clearTimeout(enterTimerRef.current);
      enterTimerRef.current = 0;
    }
    if (exitTimerRef.current) {
      window.clearTimeout(exitTimerRef.current);
      exitTimerRef.current = 0;
    }
    if (hideTimerRef.current) {
      window.clearTimeout(hideTimerRef.current);
      hideTimerRef.current = 0;
    }
  }, []);

  const dismiss = useCallback(() => {
    if (!mounted || phase === "leaving") {
      return;
    }

    clearTimers();
    const generation = generationRef.current;
    setPhase("leaving");
    exitTimerRef.current = window.setTimeout(() => {
      if (generation !== generationRef.current) {
        return;
      }
      setMounted(false);
      setPhase("");
      setDisplayText("");
    }, EXIT_MS);
  }, [clearTimers, mounted, phase]);

  const show = useCallback(
    (nextText: string, hideAfterMs = 0) => {
      const normalized = nextText.trim();
      if (!enabled || !normalized) {
        dismiss();
        return;
      }

      clearTimers();
      const generation = ++generationRef.current;
      const reopen = !mounted || phase === "leaving";
      const delay =
        persist || hideAfterMs < 0
          ? 0
          : hideAfterMs > 0
            ? hideAfterMs
            : autoHideDelay(normalized);

      setDisplayText(normalized);
      setMounted(true);
      setPhase(reopen ? "entering" : "visible");

      if (reopen) {
        enterTimerRef.current = window.setTimeout(() => {
          if (generation !== generationRef.current) {
            return;
          }
          setPhase("visible");
        }, ENTER_MS);
      }

      if (delay > 0) {
        const wait = reopen ? ENTER_MS + delay : delay;
        hideTimerRef.current = window.setTimeout(() => {
          if (generation !== generationRef.current) {
            return;
          }
          dismiss();
        }, wait);
      }
    },
    [clearTimers, dismiss, enabled, mounted, persist, phase],
  );

  const showRef = useRef(show);
  showRef.current = show;

  useEffect(() => {
    if (!enabled) {
      generationRef.current += 1;
      clearTimers();
      setMounted(false);
      setPhase("");
      setDisplayText("");
      return;
    }

    showRef.current(text, persist ? -1 : 0);
  }, [clearTimers, enabled, persist, text]);

  useEffect(() => () => clearTimers(), [clearTimers]);

  return {
    mounted,
    phase,
    displayText,
    dismiss,
    show,
    isClickable: mounted && phase === "visible",
  };
}
