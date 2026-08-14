import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { MotionConfig } from "motion/react";
import App from "./App";
import CursorSvgLab from "@/components/CursorSvgLab";
import { RecorderErrorBoundary } from "@/components/RecorderErrorBoundary";
import { TooltipProvider } from "@/components/ui/Tooltip";
import { installBrowserTestIpcMock } from "@/testHarness/browserIpcMock";
import "./App.css";
import "./accessibility.css";

installBrowserTestIpcMock();

function RootRouter() {
  const [hash, setHash] = useState(() => window.location.hash);

  useEffect(() => {
    const onHashChange = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  const isCursorLab = hash === "#cursor-lab";
  return isCursorLab ? <CursorSvgLab /> : <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MotionConfig reducedMotion="user">
      <RecorderErrorBoundary>
        <TooltipProvider>
          <RootRouter />
        </TooltipProvider>
      </RecorderErrorBoundary>
    </MotionConfig>
  </React.StrictMode>,
);
