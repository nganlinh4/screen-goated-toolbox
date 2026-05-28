import React from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import { App } from "./App";

class Boundary extends React.Component<
  { children: React.ReactNode },
  { error?: Error }
> {
  state: { error?: Error } = {};

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    // Surface render errors so the WRY-side can see them in stderr.
    console.error("[tts-playground] render error", error, info);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            padding: 20,
            color: "rgb(230,116,116)",
            fontFamily: "monospace",
            fontSize: 12,
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
          }}
        >
          <div style={{ fontWeight: 600, marginBottom: 12 }}>
            TTS Playground UI crashed
          </div>
          <div>{this.state.error.message}</div>
          <div style={{ marginTop: 12, color: "rgb(142,148,165)" }}>
            {this.state.error.stack}
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("No #root element found");
createRoot(rootEl).render(
  <React.StrictMode>
    <Boundary>
      <App />
    </Boundary>
  </React.StrictMode>,
);
