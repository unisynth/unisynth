// Context menu (change daw-solid-ts 4.2): `role="menu"` mounted only while open, one at a time. Opens on its first
// enabled item; ↑/↓ move over enabled items (wrapping), Escape or a pointer down outside closes; choosing an item
// closes, then runs it. Focus returns to the opener. Positioned at (x, y), kept inside the window.
import { For, onMount } from "solid-js";
import { render } from "solid-js/web";

/** [label, shortcut hint, action, disabled] or "-" for a separator. */
export type MenuItem = [string, string, () => void, boolean?] | "-";

let closeNow: (() => void) | null = null;

export function popMenu(x: number, y: number, items: MenuItem[], label: string, opener: Element | null = document.activeElement): void {
  closeNow?.();
  const host = document.createElement("div");
  document.body.append(host);
  let m!: HTMLDivElement;
  const out = (e: PointerEvent) => { if (!m.contains(e.target as Node)) close(); };
  const close = () => {
    if (closeNow !== close) return;
    closeNow = null; dispose(); host.remove(); document.removeEventListener("pointerdown", out, true);
    (opener as HTMLElement | null)?.focus?.({ preventScroll: true });
  };
  const keys = (e: KeyboardEvent) => {
    const bs = Array.from(m.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")), i = bs.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") { e.preventDefault(); close(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); bs[(i + 1) % bs.length].focus(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); bs[(i - 1 + bs.length) % bs.length].focus(); }
  };
  const dispose = render(() => {
    onMount(() => {
      const r = m.getBoundingClientRect();
      m.style.left = Math.min(x, innerWidth - r.width - 4) + "px"; m.style.top = Math.min(y, innerHeight - r.height - 4) + "px";
      m.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    });
    return (
      <div ref={m} class="menu" role="menu" aria-label={label} onKeyDown={keys}>
        <For each={items}>{(it) => it === "-" ? <hr /> : (
          <button role="menuitem" disabled={!!it[3]} onClick={() => { close(); it[2](); }}>
            <span>{it[0]}</span>{it[1] ? <kbd>{it[1]}</kbd> : null}
          </button>
        )}</For>
      </div>
    );
  }, host);
  closeNow = close;
  setTimeout(() => { if (closeNow === close) document.addEventListener("pointerdown", out, true); });
}
