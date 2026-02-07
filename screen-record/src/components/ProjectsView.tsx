import { useState, useRef, useEffect } from 'react';
import { Video, Trash2, Play, X } from 'lucide-react';
import { Project } from '@/types/video';
import { projectManager } from '@/lib/projectManager';
import { useSettings } from '@/hooks/useSettings';

interface ProjectsViewProps {
  projects: Omit<Project, 'videoBlob'>[];
  onLoadProject: (projectId: string) => void | Promise<void>;
  onProjectsChange: () => void;
  onClose: () => void;
  currentProjectId?: string | null;
  restoreImage?: string | null;
}

function formatDuration(s: number): string {
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, '0')}`;
}

/** Compute the object-fit:contain rect for an aspect ratio inside a container. */
function containRect(
  cw: number, ch: number, nw: number, nh: number
): { left: number; top: number; width: number; height: number } {
  const ca = cw / ch, na = nw / nh;
  let w: number, h: number;
  if (ca > na) { h = ch; w = h * na; }
  else         { w = cw; h = w / na; }
  return { left: (cw - w) / 2, top: (ch - h) / 2, width: w, height: h };
}

export function ProjectsView({ projects, onLoadProject, onProjectsChange, onClose, currentProjectId, restoreImage }: ProjectsViewProps) {
  const [editingNameId, setEditingNameId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [animatingId, setAnimatingId] = useState<string | null>(null);
  const [isRestoring, setIsRestoring] = useState(() => !!restoreImage && !!currentProjectId);
  const animatingRef = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const animatedCloseRef = useRef<() => void>(() => {});
  const { t } = useSettings();

  // Restore animation: shrink current video frame into its card position
  useEffect(() => {
    if (!restoreImage || !currentProjectId || !containerRef.current) {
      setIsRestoring(false);
      return;
    }

    const container = containerRef.current;

    requestAnimationFrame(() => {
      const containerRect = container.getBoundingClientRect();

      const img = new Image();
      img.src = restoreImage;

      const runAnimation = () => {
        const natW = img.naturalWidth || 16;
        const natH = img.naturalHeight || 9;
        const source = containRect(containerRect.width, containerRect.height, natW, natH);

        const card = container.querySelector(`[data-project-id="${currentProjectId}"]`) as HTMLElement | null;
        if (!card) { setIsRestoring(false); return; }

        // Ensure card is visible in scroll container
        card.scrollIntoView({ block: 'nearest', behavior: 'instant' as ScrollBehavior });

        const thumbArea = card.querySelector('.aspect-video') as HTMLElement | null;
        if (!thumbArea) { setIsRestoring(false); return; }

        // Re-measure after potential scroll
        const freshContainerRect = container.getBoundingClientRect();
        const thumbRect = thumbArea.getBoundingClientRect();
        const thumbRelLeft = thumbRect.left - freshContainerRect.left;
        const thumbRelTop = thumbRect.top - freshContainerRect.top;

        // Clone at canvas (source) position
        const clone = document.createElement('div');
        clone.style.cssText = `
          position: absolute; z-index: 60; pointer-events: none;
          left: ${source.left}px; top: ${source.top}px;
          width: ${source.width}px; height: ${source.height}px;
          overflow: hidden; transform-origin: 0 0;
          will-change: transform;
        `;
        const imgEl = document.createElement('img');
        imgEl.src = restoreImage;
        imgEl.style.cssText = 'width: 100%; height: 100%; object-fit: cover;';
        clone.appendChild(imgEl);
        container.appendChild(clone);

        // Animate source → target (reverse of expand)
        const dx = thumbRelLeft - source.left;
        const dy = thumbRelTop - source.top;
        const sx = thumbRect.width / source.width;
        const sy = thumbRect.height / source.height;

        clone.animate([
          { transform: 'none', borderRadius: '0px' },
          { transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`, borderRadius: '8px' }
        ], {
          duration: 500,
          easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
          fill: 'forwards'
        }).onfinish = () => {
          clone.animate(
            [{ opacity: 1 }, { opacity: 0 }],
            { duration: 150, fill: 'forwards' }
          ).onfinish = () => clone.remove();
        };

        // Fade in grid content
        setTimeout(() => setIsRestoring(false), 50);
      };

      if (img.complete) runAnimation();
      else {
        img.onload = runAnimation;
        img.onerror = () => setIsRestoring(false);
      }
    });
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !animatingRef.current) animatedCloseRef.current();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // External close requests (toggle button dispatches this event)
  useEffect(() => {
    const handler = () => animatedCloseRef.current();
    window.addEventListener('sr-close-projects', handler);
    return () => window.removeEventListener('sr-close-projects', handler);
  }, []);

  const handleRename = async (id: string) => {
    if (!renameValue.trim()) return;
    const project = await projectManager.loadProject(id);
    if (project) {
      await projectManager.updateProject(id, { ...project, name: renameValue.trim() });
      onProjectsChange();
    }
    setEditingNameId(null);
  };

  const handleProjectClick = (projectId: string, e: React.MouseEvent<HTMLDivElement>) => {
    if (animatingRef.current) return;

    const thumbnailImg = e.currentTarget.querySelector('img') as HTMLImageElement | null;
    const container = containerRef.current;
    // Portal target: the preview area parent that survives after this component unmounts
    const portalTarget = container?.parentElement;

    if (!thumbnailImg || !container || !portalTarget) {
      onLoadProject(projectId);
      return;
    }

    const parentRect = portalTarget.getBoundingClientRect();
    const thumbRect = thumbnailImg.getBoundingClientRect();

    // Thumbnail natural dimensions = canvas pixel dimensions
    const natW = thumbnailImg.naturalWidth || 16;
    const natH = thumbnailImg.naturalHeight || 9;

    // Where the canvas will actually render (object-fit:contain, flex-centered)
    const target = containRect(parentRect.width, parentRect.height, natW, natH);

    // Thumbnail position relative to portal target
    const thumbRelLeft = thumbRect.left - parentRect.left;
    const thumbRelTop = thumbRect.top - parentRect.top;

    // FLIP: source → target
    const dx = thumbRelLeft - target.left;
    const dy = thumbRelTop - target.top;
    const sx = thumbRect.width / target.width;
    const sy = thumbRect.height / target.height;

    // Background fill — covers the whole parent so no flash when ProjectsView unmounts
    const bg = document.createElement('div');
    bg.style.cssText = 'position:absolute;inset:0;z-index:59;background:var(--surface);pointer-events:none;';
    portalTarget.appendChild(bg);

    // Clone lives in the PARENT so it persists after this component unmounts
    const clone = document.createElement('div');
    clone.style.cssText = `
      position: absolute; z-index: 60; pointer-events: none;
      left: ${target.left}px; top: ${target.top}px;
      width: ${target.width}px; height: ${target.height}px;
      overflow: hidden; transform-origin: 0 0;
      will-change: transform;
    `;
    const img = document.createElement('img');
    img.src = thumbnailImg.src;
    img.style.cssText = 'width: 100%; height: 100%; object-fit: cover;';
    clone.appendChild(img);
    portalTarget.appendChild(clone);

    animatingRef.current = true;
    setAnimatingId(projectId);

    const fadeOut = () => {
      clone.animate(
        [{ opacity: 1 }, { opacity: 0 }],
        { duration: 200, fill: 'forwards' }
      ).onfinish = () => clone.remove();
      bg.animate(
        [{ opacity: 1 }, { opacity: 0 }],
        { duration: 200, fill: 'forwards' }
      ).onfinish = () => bg.remove();
    };

    clone.animate([
      { transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`, borderRadius: '8px' },
      { transform: 'none', borderRadius: '0px' }
    ], {
      duration: 500,
      easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
      fill: 'forwards'
    }).onfinish = () => {
      animatingRef.current = false;

      // Await the full project load, then give React time to commit + paint before dissolving
      Promise.resolve(onLoadProject(projectId)).then(() => {
        // renderImmediate already drew the frame on the canvas inside handleLoadProject.
        // Wait 100ms to ensure React effects have flushed, then fade on next frame.
        setTimeout(() => requestAnimationFrame(fadeOut), 100);
      });
    };
  };

  // Animated close: expand current project's card → canvas, then unmount
  const handleAnimatedClose = () => {
    if (animatingRef.current) return;

    const container = containerRef.current;
    const portalTarget = container?.parentElement;

    if (!currentProjectId || !container || !portalTarget) {
      onClose();
      return;
    }

    const card = container.querySelector(`[data-project-id="${currentProjectId}"]`) as HTMLElement | null;
    const thumbnailImg = card?.querySelector('.aspect-video img') as HTMLImageElement | null;

    if (!card || !thumbnailImg) {
      onClose();
      return;
    }

    card.scrollIntoView({ block: 'nearest', behavior: 'instant' as ScrollBehavior });

    const parentRect = portalTarget.getBoundingClientRect();
    const thumbRect = thumbnailImg.getBoundingClientRect();
    const natW = thumbnailImg.naturalWidth || 16;
    const natH = thumbnailImg.naturalHeight || 9;
    const target = containRect(parentRect.width, parentRect.height, natW, natH);
    const thumbRelLeft = thumbRect.left - parentRect.left;
    const thumbRelTop = thumbRect.top - parentRect.top;

    const dx = thumbRelLeft - target.left;
    const dy = thumbRelTop - target.top;
    const sx = thumbRect.width / target.width;
    const sy = thumbRect.height / target.height;

    const bg = document.createElement('div');
    bg.style.cssText = 'position:absolute;inset:0;z-index:59;background:var(--surface);pointer-events:none;';
    portalTarget.appendChild(bg);

    const clone = document.createElement('div');
    clone.style.cssText = `
      position: absolute; z-index: 60; pointer-events: none;
      left: ${target.left}px; top: ${target.top}px;
      width: ${target.width}px; height: ${target.height}px;
      overflow: hidden; transform-origin: 0 0;
      will-change: transform;
    `;
    const img = document.createElement('img');
    img.src = thumbnailImg.src;
    img.style.cssText = 'width: 100%; height: 100%; object-fit: cover;';
    clone.appendChild(img);
    portalTarget.appendChild(clone);

    animatingRef.current = true;
    setAnimatingId(currentProjectId);

    const fadeOut = () => {
      clone.animate(
        [{ opacity: 1 }, { opacity: 0 }],
        { duration: 200, fill: 'forwards' }
      ).onfinish = () => clone.remove();
      bg.animate(
        [{ opacity: 1 }, { opacity: 0 }],
        { duration: 200, fill: 'forwards' }
      ).onfinish = () => bg.remove();
    };

    clone.animate([
      { transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`, borderRadius: '8px' },
      { transform: 'none', borderRadius: '0px' }
    ], {
      duration: 500,
      easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
      fill: 'forwards'
    }).onfinish = () => {
      animatingRef.current = false;
      onClose();
      setTimeout(() => requestAnimationFrame(fadeOut), 100);
    };
  };

  // Keep ref current so event listeners always call the latest version
  animatedCloseRef.current = handleAnimatedClose;

  return (
    <div ref={containerRef} className="absolute inset-0 z-20 overflow-hidden rounded-xl">
      {/* Solid background — stays opaque while this component lives */}
      <div className="absolute inset-0 bg-[var(--surface)]" />

      {/* Content fades out during thumbnail expansion */}
      <div className={`relative flex flex-col h-full transition-opacity ${
        animatingId ? 'opacity-0 duration-200' : isRestoring ? 'opacity-0' : 'opacity-100 duration-300'
      }`}>
        {/* Header */}
        <div className="flex justify-between items-center px-4 py-3 flex-shrink-0">
          <h3 className="text-sm font-medium text-[var(--on-surface)]">{t.projects}</h3>
          <div className="flex items-center gap-3 flex-shrink-0 whitespace-nowrap">
            <span className="text-[10px] text-[var(--outline)]">{t.max}</span>
            <input
              type="range" min="10" max="100" value={projectManager.getLimit()}
              onChange={(e) => { projectManager.setLimit(parseInt(e.target.value)); onProjectsChange(); }}
              className="w-16"
            />
            <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-5">{projectManager.getLimit()}</span>
            <button onClick={handleAnimatedClose} className="p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Grid */}
        <div className="flex-1 min-h-0 overflow-y-auto thin-scrollbar px-4 pb-4">
          {projects.length === 0 ? (
            <div className="flex items-center justify-center h-full text-xs text-[var(--outline)]">{t.noProjectsYet}</div>
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
              {projects.map((project) => (
                <div key={project.id} data-project-id={project.id} className="group relative bg-[var(--surface-container)] border border-[var(--glass-border)] rounded-lg overflow-hidden hover:border-[var(--outline)] transition-colors">
                  <div
                    className="aspect-video bg-[var(--surface-container-high)] relative cursor-pointer overflow-hidden"
                    onClick={(e) => handleProjectClick(project.id, e)}
                  >
                    {project.thumbnail ? (
                      <img src={project.thumbnail} className="w-full h-full object-cover" alt="" />
                    ) : (
                      <div className="w-full h-full flex items-center justify-center">
                        <Video className="w-6 h-6 text-[var(--outline-variant)]" />
                      </div>
                    )}
                    <div className="absolute inset-0 bg-black/0 group-hover:bg-black/30 transition-colors flex items-center justify-center">
                      <Play className="w-7 h-7 text-white opacity-0 group-hover:opacity-90 transition-opacity" />
                    </div>
                    {project.duration != null && project.duration > 0 && (
                      <span
                        className="absolute bottom-2 right-2.5 text-white tabular-nums pointer-events-none"
                        style={{
                          fontSize: '1.35rem',
                          fontVariationSettings: "'wght' 700, 'ROND' 100",
                          textShadow: '0 1px 4px rgba(0,0,0,0.7), 0 0 12px rgba(0,0,0,0.4)',
                          letterSpacing: '-0.02em',
                        }}
                      >
                        {formatDuration(project.duration)}
                      </span>
                    )}
                  </div>
                  <div className="p-2 flex items-start justify-between gap-1">
                    <div className="min-w-0 flex-1">
                      {editingNameId === project.id ? (
                        <input
                          autoFocus
                          className="bg-transparent border-b border-[var(--primary-color)] text-[var(--on-surface)] text-xs w-full outline-none py-0.5"
                          value={renameValue}
                          onChange={(e) => setRenameValue(e.target.value)}
                          onBlur={() => handleRename(project.id)}
                          onKeyDown={(e) => e.key === 'Enter' && handleRename(project.id)}
                        />
                      ) : (
                        <p
                          className="text-xs text-[var(--on-surface)] truncate cursor-pointer hover:text-[var(--primary-color)] transition-colors"
                          onClick={() => { setEditingNameId(project.id); setRenameValue(project.name); }}
                        >
                          {project.name}
                        </p>
                      )}
                      <p className="text-[10px] text-[var(--outline)] mt-0.5">{new Date(project.lastModified).toLocaleDateString()}</p>
                    </div>
                    <button
                      onClick={async () => { await projectManager.deleteProject(project.id); onProjectsChange(); }}
                      className="text-[var(--outline)] hover:text-red-400 transition-colors opacity-0 group-hover:opacity-100 p-0.5 flex-shrink-0"
                    >
                      <Trash2 className="w-3 h-3" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
