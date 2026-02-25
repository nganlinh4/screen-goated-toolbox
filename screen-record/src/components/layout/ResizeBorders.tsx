import React from "react";

const ipc = (msg: string) => (window as any).ipc.postMessage(msg);

export function ResizeBorders() {
  const resize = (dir: string) => (e: React.MouseEvent) => { e.preventDefault(); ipc(`resize_${dir}`); };
  return (
    <>
      {/* Edges: left / right full-height, bottom full-width (top handled by Header) */}
      <div className="resize-border-left fixed top-0 left-0 bottom-0 w-[6px] z-50 cursor-ew-resize" onMouseDown={resize('w')} />
      <div className="resize-border-right fixed top-0 right-0 bottom-0 w-[6px] z-50 cursor-ew-resize" onMouseDown={resize('e')} />
      <div className="resize-border-bottom fixed bottom-0 left-[14px] right-[14px] h-[6px] z-50 cursor-ns-resize" onMouseDown={resize('s')} />
      {/* Corners */}
      <div className="resize-corner-sw fixed bottom-0 left-0 w-[14px] h-[14px] z-50 cursor-nesw-resize" onMouseDown={resize('sw')} />
      <div className="resize-corner-se fixed bottom-0 right-0 w-[14px] h-[14px] z-50 cursor-nwse-resize" onMouseDown={resize('se')} />
    </>
  );
}
