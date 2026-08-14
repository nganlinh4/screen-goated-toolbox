import { Component, type ErrorInfo, type ReactNode } from "react";
import { getTranslations, type Translations } from "@/i18n";

interface RecorderErrorBoundaryProps {
  children: ReactNode;
}

interface RecorderErrorBoundaryState {
  failed: boolean;
}

export class RecorderErrorBoundary extends Component<
  RecorderErrorBoundaryProps,
  RecorderErrorBoundaryState
> {
  state: RecorderErrorBoundaryState = { failed: false };

  static getDerivedStateFromError(): RecorderErrorBoundaryState {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Recorder UI crashed", error, info.componentStack);
  }

  private translations(): Translations {
    const documentLanguage = document.documentElement.lang;
    const language = (window as Window & { __SR_INITIAL_LANG__?: string })
      .__SR_INITIAL_LANG__;
    return getTranslations(documentLanguage || language || "en");
  }

  render() {
    if (!this.state.failed) return this.props.children;
    const t = this.translations();
    return (
      <main
        className="recorder-error-boundary flex min-h-screen items-center justify-center bg-[var(--surface)] p-6 text-[var(--on-surface)]"
        role="alert"
        aria-live="assertive"
      >
        <section className="ui-surface-elevated max-w-md rounded-2xl p-6 text-center">
          <h1 className="text-base font-semibold">{t.unexpectedError}</h1>
          <button
            type="button"
            className="ui-action-button mt-5 rounded-xl px-4 py-2 text-sm"
            data-emphasis="strong"
            data-tone="primary"
            onClick={() => window.location.reload()}
          >
            {t.reloadRecorder}
          </button>
        </section>
      </main>
    );
  }
}
