import { css, html, LitElement } from 'lit';
import { customElement, state, property } from 'lit/decorators.js';
import { LOCALES, Lang } from '../utils/Locales';

@customElement('onboarding-popup')
export class OnboardingPopup extends LitElement {
    static override styles = css`
    :host {
      font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
    }
    .overlay {
      position: fixed;
      top: 0;
      left: 0;
      width: 100%;
      height: 100%;
      background: rgba(0, 0, 0, 0.6);
      backdrop-filter: blur(8px);
      z-index: 2000;
      display: flex;
      align-items: center;
      justify-content: center;
      animation: fadeIn 0.4s cubic-bezier(0.2, 0.0, 0, 1.0);
    }
    .popup {
      background: linear-gradient(145deg, rgba(30, 30, 30, 0.95), rgba(15, 15, 15, 0.98));
      border: 1px solid rgba(255, 255, 255, 0.15);
      border-radius: 20px;
      padding: 32px 40px;
      max-width: 480px;
      width: 90%;
      box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
      display: flex;
      flex-direction: column;
      gap: 20px;
      transform: translateY(0);
      animation: slideUp 0.4s cubic-bezier(0.2, 0.0, 0, 1.0);
      color: #fff;
    }
    .title {
      font-size: 20px;
      font-weight: 600;
      margin: 0;
      color: #fff;
      display: flex;
      align-items: center;
      gap: 10px;
    }
    /* Simple info icon using CSS/Unicode or SVG. SVG is better. */
    .icon {
        color: #bb86fc;
        width: 24px;
        height: 24px;
    }
    .message {
      font-size: 16px;
      line-height: 1.6;
      opacity: 0.9;
      color: #e0e0e0;
      margin: 0;
    }
    .btn {
      align-self: flex-end;
      background: linear-gradient(135deg, #6200ea, #7c4dff);
      color: #fff;
      border: none;
      padding: 10px 24px;
      border-radius: 8px;
      font-size: 15px;
      font-weight: 600;
      cursor: pointer;
      transition: all 0.2s ease;
      font-family: inherit;
      box-shadow: 0 4px 12px rgba(98, 0, 234, 0.3);
    }
    .btn:hover {
      filter: brightness(1.1);
      transform: translateY(-1px);
      box-shadow: 0 6px 16px rgba(98, 0, 234, 0.4);
    }
    .btn:active {
      transform: translateY(0);
    }
    @keyframes fadeIn {
      from { opacity: 0; }
      to { opacity: 1; }
    }
    @keyframes slideUp {
      from { transform: translateY(20px); opacity: 0; }
      to { transform: translateY(0); opacity: 1; }
    }
  `;

    @property({ type: String }) lang: string = 'en';
    @state() private isVisible = false;

    connectedCallback() {
        super.connectedCallback();
        const seen = localStorage.getItem('pdj_onboarding_seen');
        if (!seen) {
            this.isVisible = true;
        }
    }

    private dismiss() {
        localStorage.setItem('pdj_onboarding_seen', 'true');
        this.isVisible = false;
    }

    render() {
        if (!this.isVisible) return html``;
        const t = LOCALES[this.lang as Lang];

        return html`
      <div class="overlay" @click=${this.dismiss}>
        <div class="popup" @click=${(e: Event) => e.stopPropagation()}>
          <h2 class="title">
            <svg class="icon" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"/></svg>
            ${t.onboarding_title}
          </h2>
          <p class="message">
            ${t.onboarding_msg}
          </p>
          <button class="btn" @click=${this.dismiss}>${t.onboarding_btn}</button>
        </div>
      </div>
    `;
    }
}
