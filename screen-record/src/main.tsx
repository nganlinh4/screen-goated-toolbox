import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import CursorSvgLab from "@/components/CursorSvgLab";
import "./App.css";

console.log("[SR-BUILD] bundle=v15-sgtcool-labspacefix cursor-debug-always-on");

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
    <RootRouter />
  </React.StrictMode>,
);
