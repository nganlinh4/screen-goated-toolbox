import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { MotionConfig } from "motion/react";
import App from "./App";
import CursorSvgLab from "@/components/CursorSvgLab";
import { RecorderErrorBoundary } from "@/components/RecorderErrorBoundary";
import { TooltipProvider } from "@/components/ui/Tooltip";
import { installBrowserTestIpcMock } from "@/testHarness/browserIpcMock";
import { reportStartupMilestone } from "@/lib/startupTelemetry";
import "./App.css";
import "./accessibility.css";

installBrowserTestIpcMock();
reportStartupMilestone("frontend-module-evaluated");

function StartupTelemetry() {
  useEffect(() => {
    reportStartupMilestone("react-committed");
    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        reportStartupMilestone("first-visible-frame");
      });
    });
    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, []);
  return null;
}

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
          <StartupTelemetry />
          <RootRouter />
        </TooltipProvider>
      </RecorderErrorBoundary>
    </MotionConfig>
  </React.StrictMode>,
);
