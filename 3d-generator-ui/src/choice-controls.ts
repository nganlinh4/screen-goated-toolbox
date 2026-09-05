import "./choice-controls.css";

const controls = new WeakMap<HTMLSelectElement, HTMLElement>();

/** Retain the select value contract while rendering app-styled choices. */
export function syncChoiceControl(select: HTMLSelectElement) {
  let group = controls.get(select);
  if (!group) {
    group = document.createElement("div");
    group.className = "choice-options";
    group.setAttribute("role", "group");
    for (const option of select.options) {
      const button = document.createElement("button");
      button.type = "button";
      button.addEventListener("click", () => {
        if (select.disabled || option.disabled || option.hidden) return;
        select.value = option.value;
        select.dispatchEvent(new Event("change", { bubbles: true }));
        syncChoiceControl(select);
      });
      group.append(button);
    }
    select.hidden = true;
    select.after(group);
    controls.set(select, group);
  }
  group.setAttribute("aria-label", select.getAttribute("aria-label") || "");
  [...group.querySelectorAll<HTMLButtonElement>("button")].forEach((button, index) => {
    const option = select.options[index];
    button.textContent = option.textContent;
    button.hidden = option.hidden;
    button.disabled = select.disabled || option.disabled;
    button.setAttribute("aria-pressed", String(select.value === option.value));
  });
}
