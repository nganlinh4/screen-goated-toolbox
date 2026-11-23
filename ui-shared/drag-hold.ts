/**
 * Keeps a control the user is dragging from being overwritten by host state.
 * A host that pushes its whole state after each step (or on every playback
 * tick) delivers echoes of earlier steps while the pointer has already moved
 * on; presenting them would yank the control back under the pointer. Host
 * state wins again once the gesture ends.
 */
export class DragHold<T> {
  private held: { value: T } | undefined;

  get active(): boolean {
    return this.held !== undefined;
  }

  hold(value: T): void {
    this.held = { value };
  }

  release(): void {
    this.held = undefined;
  }

  present(hostValue: T): T {
    return this.held ? this.held.value : hostValue;
  }
}
