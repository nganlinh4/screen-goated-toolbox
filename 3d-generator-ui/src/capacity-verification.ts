import { t } from "./i18n";
import "./capacity-verification.css";

type Status = { pendingCount: number; running: boolean; failed: boolean };
type Invoke = <T>(command: string) => Promise<T>;

export class CapacityVerification {
  private status: Status = { pendingCount: 0, running: false, failed: false };
  private disposed = false;
  private opening = false;
  private readonly chip = document.createElement("button");
  private readonly dialog = document.createElement("dialog");
  private readonly heading = document.createElement("h2");
  private readonly copy = document.createElement("p");
  private readonly reassurance = document.createElement("p");
  private readonly feedback = document.createElement("p");
  private readonly verify = document.createElement("button");
  private readonly later = document.createElement("button");

  constructor(private readonly invoke: Invoke) {
    this.chip.type = "button";
    this.chip.className = "capacity-verification-chip";
    this.chip.hidden = true;
    this.chip.setAttribute("aria-haspopup", "dialog");
    document.querySelector(".identity")?.append(this.chip);
    this.dialog.className = "capacity-verification-dialog";
    this.heading.id = "capacity-verification-title";
    this.copy.id = "capacity-verification-description";
    this.dialog.setAttribute("aria-labelledby", this.heading.id);
    this.dialog.setAttribute("aria-describedby", this.copy.id);
    this.reassurance.className = "capacity-verification-reassurance";
    this.feedback.className = "capacity-verification-feedback";
    this.feedback.setAttribute("role", "status");
    const actions = document.createElement("div");
    actions.className = "capacity-verification-actions";
    this.later.type = this.verify.type = "button";
    this.verify.className = "capacity-verification-primary";
    actions.append(this.later, this.verify);
    this.dialog.append(this.heading, this.copy, this.reassurance, this.feedback, actions);
    document.body.append(this.dialog);
    this.chip.addEventListener("click", () => {
      this.render();
      this.dialog.showModal();
      this.verify.focus();
    });
    this.later.addEventListener("click", () => this.dialog.close());
    this.verify.addEventListener("click", () => void this.start());
    window.addEventListener("creation-verification-changed", this.read);
    window.addEventListener("focus", this.refresh);
    window.addEventListener("pagehide", () => this.dispose(), { once: true });
    this.render();
    this.refresh();
    void this.read();
  }

  private readonly refresh = () => {
    if (!this.disposed) void this.invoke("refresh_capacity_verification").catch(() => undefined);
  };

  private readonly read = async () => {
    try {
      const value = await this.invoke<Status>("capacity_verification_status");
      if (this.disposed || !value || ![0, 1].includes(value.pendingCount)
        || typeof value.running !== "boolean" || typeof value.failed !== "boolean") return;
      this.status = value;
      this.render();
    } catch { /* An unavailable status cannot create a verification request. */ }
  };

  private async start() {
    if (this.opening || this.status.running || !this.status.pendingCount) return;
    this.opening = true;
    this.render();
    try {
      await this.invoke("verify_capacity");
      await this.read();
    } catch {
      this.status.failed = true;
    } finally {
      this.opening = false;
      this.render();
    }
  }

  private render() {
    const busy = this.opening || this.status.running;
    this.chip.hidden = !this.status.pendingCount && !busy;
    this.chip.textContent = busy ? t("capacityChecking") : t("capacityActionNeeded");
    this.heading.textContent = t("capacityTitle");
    this.copy.textContent = t("capacityDescription");
    this.reassurance.textContent = t("capacityReassurance");
    this.verify.textContent = busy ? t("capacityChecking") : t("capacityVerify");
    this.verify.disabled = busy || !this.status.pendingCount;
    this.later.textContent = t("capacityLater");
    this.feedback.textContent = this.status.failed ? t("capacityFailed")
      : busy ? t("completeSecureWindow") : "";
    if (!this.status.pendingCount && !busy && !this.status.failed && this.dialog.open) {
      this.dialog.close();
    }
  }

  private dispose() {
    this.disposed = true;
    window.removeEventListener("creation-verification-changed", this.read);
    window.removeEventListener("focus", this.refresh);
    this.dialog.remove();
    this.chip.remove();
  }
}
