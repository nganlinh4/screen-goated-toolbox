import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { startProjectRestoreAnimation } from "@/components/projectsRestoreAnimation";

interface FakeAnimation {
  onfinish: (() => void) | null;
  cancel: ReturnType<typeof vi.fn>;
}

function setup() {
  const container = document.createElement("div");
  container.innerHTML = '<div data-project-id="p1"><div class="project-thumbnail"></div></div>';
  document.body.appendChild(container);
  const card = container.querySelector<HTMLElement>("[data-project-id]")!;
  card.scrollIntoView = vi.fn();
  const animations: FakeAnimation[] = [];
  HTMLElement.prototype.animate = vi.fn(() => {
    const animation: FakeAnimation = { onfinish: null, cancel: vi.fn() };
    animations.push(animation);
    return animation as unknown as Animation;
  });
  const onRevealContent = vi.fn();
  const onShowCard = vi.fn();
  const dispose = startProjectRestoreAnimation({
    container,
    projectId: "p1",
    restoreImage: "data:image/png;base64,AAAA",
    snapshotCanvas: { left: 0, top: 44, width: 800, height: 450 } as never,
    onRevealContent,
    onShowCard,
  });
  const clones = () => document.querySelectorAll(".projects-restore-clone").length;
  return { dispose, animations, onRevealContent, onShowCard, clones, container };
}

describe("project restore animation", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame", "cancelAnimationFrame"] });
    // jsdom never loads images; treat the snapshot as decoded so the start chain runs.
    vi.spyOn(HTMLImageElement.prototype, "complete", "get").mockReturnValue(true);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    document.body.innerHTML = "";
  });

  it("leaves no clone and starts nothing when disposed before the animation begins", () => {
    const { dispose, animations, onShowCard, clones } = setup();
    expect(clones()).toBe(1);
    dispose();
    vi.runAllTimers();
    expect(clones()).toBe(0);
    expect(animations).toHaveLength(0);
    expect(onShowCard).not.toHaveBeenCalled();
  });

  it("cancels an in-flight flip and removes its clone when disposed mid-animation", () => {
    const { dispose, animations, onShowCard, clones } = setup();
    vi.runAllTimers();
    expect(animations).toHaveLength(1);
    dispose();
    expect(animations[0].cancel).toHaveBeenCalledOnce();
    expect(animations[0].onfinish).toBeNull();
    expect(clones()).toBe(0);
    expect(onShowCard).not.toHaveBeenCalled();
  });

  it("still completes the restore when left running", () => {
    const { animations, onRevealContent, onShowCard, clones } = setup();
    expect(onRevealContent).toHaveBeenCalledOnce();
    vi.runAllTimers();
    animations[0].onfinish?.();
    expect(onShowCard).toHaveBeenCalledOnce();
    vi.runAllTimers();
    animations[1].onfinish?.();
    expect(clones()).toBe(0);
  });
});
