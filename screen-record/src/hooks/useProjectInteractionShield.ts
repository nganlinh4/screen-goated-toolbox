import {
  useState,
  useCallback,
  useRef,
  useEffect,
  type RefObject,
  type MutableRefObject,
} from "react";

interface UseProjectInteractionShieldParams {
  showProjectsDialog: boolean;
  previewContainerRef: RefObject<HTMLDivElement | null>;
  /** Optional externally-created ref; if omitted a new ref is created internally. */
  isProjectTransitionRef?: MutableRefObject<boolean>;
}

export function useProjectInteractionShield({
  showProjectsDialog,
  previewContainerRef,
  isProjectTransitionRef: externalTransitionRef,
}: UseProjectInteractionShieldParams) {
  const [isProjectInteractionShieldVisible, setIsProjectInteractionShieldVisible] =
    useState(false);
  const internalTransitionRef = useRef(false);
  const isProjectTransitionRef = externalTransitionRef ?? internalTransitionRef;
  const projectInteractionShieldReleaseRef = useRef<(() => void) | null>(null);
  const projectInteractionBlockCleanupRef = useRef<(() => void) | null>(null);
  const wasProjectsDialogOpenRef = useRef(false);

  const beginProjectInteractionShield = useCallback(() => {
    projectInteractionShieldReleaseRef.current?.();
    projectInteractionBlockCleanupRef.current?.();
    isProjectTransitionRef.current = true;
    setIsProjectInteractionShieldVisible(true);

    const eventNames = [
      "pointerdown",
      "pointerup",
      "pointermove",
      "mousedown",
      "mouseup",
      "mousemove",
      "click",
    ] as const;

    const swallow = (event: Event) => {
      const target = event.target;
      if (target instanceof Element && target.closest(".projects-view")) {
        return;
      }
      if ("cancelable" in event && event.cancelable) {
        event.preventDefault();
      }
      event.stopPropagation();
      (
        event as Event & {
          stopImmediatePropagation?: () => void;
        }
      ).stopImmediatePropagation?.();
    };

    const cleanup = () => {
      eventNames.forEach((eventName) => {
        window.removeEventListener(eventName, swallow, true);
      });
      if (projectInteractionBlockCleanupRef.current === cleanup) {
        projectInteractionBlockCleanupRef.current = null;
      }
    };

    eventNames.forEach((eventName) => {
      window.addEventListener(eventName, swallow, true);
    });
    projectInteractionBlockCleanupRef.current = cleanup;
  }, []);

  const abortEditorInteractions = useCallback(() => {
    const activeElement = document.activeElement;
    if (
      activeElement instanceof HTMLElement &&
      typeof activeElement.blur === "function" &&
      !(activeElement instanceof Element && activeElement.closest(".projects-view"))
    ) {
      activeElement.blur();
    }
    window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    if (typeof PointerEvent !== "undefined") {
      window.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
      window.dispatchEvent(
        new PointerEvent("pointercancel", { bubbles: true }),
      );
    }
    window.dispatchEvent(new CustomEvent("sr-abort-editor-interactions"));
  }, []);

  const armProjectInteractionShieldRelease = useCallback(() => {
    projectInteractionShieldReleaseRef.current?.();

    let released = false;
    let timeoutId: number | null = null;

    const cleanup = () => {
      window.removeEventListener("pointerup", release, true);
      window.removeEventListener("mouseup", release, true);
      window.removeEventListener("click", release, true);
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
      if (projectInteractionShieldReleaseRef.current === cleanup) {
        projectInteractionShieldReleaseRef.current = null;
      }
    };

    const release = () => {
      if (released) return;
      released = true;
      cleanup();
      projectInteractionBlockCleanupRef.current?.();
      isProjectTransitionRef.current = false;
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          setIsProjectInteractionShieldVisible(false);
        });
      });
    };

    timeoutId = window.setTimeout(release, 420);
    window.addEventListener("pointerup", release, true);
    window.addEventListener("mouseup", release, true);
    window.addEventListener("click", release, true);
    projectInteractionShieldReleaseRef.current = cleanup;
  }, []);

  useEffect(() => {
    return () => {
      projectInteractionShieldReleaseRef.current?.();
      projectInteractionBlockCleanupRef.current?.();
    };
  }, []);

  useEffect(() => {
    if (showProjectsDialog) {
      wasProjectsDialogOpenRef.current = true;
      return;
    }
    if (!wasProjectsDialogOpenRef.current) return;
    wasProjectsDialogOpenRef.current = false;

    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        window.focus();
        previewContainerRef.current?.focus({ preventScroll: true });
      });
    });
  }, [showProjectsDialog, previewContainerRef]);

  return {
    isProjectInteractionShieldVisible,
    setIsProjectInteractionShieldVisible,
    isProjectTransitionRef,
    projectInteractionShieldReleaseRef,
    projectInteractionBlockCleanupRef,
    beginProjectInteractionShield,
    abortEditorInteractions,
    armProjectInteractionShieldRelease,
  };
}
