const NON_TEXT_INPUTS = ["range", "checkbox", "radio", "button"];

/** True when the event target is a field that consumes typed characters (so global shortcuts must not fire). */
export function isTypingTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  return (
    el instanceof HTMLTextAreaElement ||
    el instanceof HTMLSelectElement ||
    (el instanceof HTMLInputElement && !NON_TEXT_INPUTS.includes(el.type)) ||
    el.isContentEditable === true
  );
}
