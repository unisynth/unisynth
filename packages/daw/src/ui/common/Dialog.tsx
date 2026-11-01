// Modal dialog (change daw-solid-ts 4.2): `<dialog class="dlg">` mounted only while open, one at a time (opening one
// closes the previous). Buttons run their action; an action returning false keeps the dialog open (e.g. Save failed).
// Escape closes (native cancel). Focus returns to the opener (native showModal/close).
import { For, type JSX } from "solid-js";
import { render } from "solid-js/web";

export interface DialogButton {
  label: string;
  primary?: boolean;
  /** Return false (or a promise of false) to keep the dialog open. */
  run?: () => unknown;
}
export interface DialogHandle { el: HTMLDialogElement; close(): void }

let openNow: DialogHandle | null = null;

export function openDialog(title: string, body: () => JSX.Element, buttons: DialogButton[]): DialogHandle {
  openNow?.close();
  const host = document.createElement("div");
  document.body.append(host);
  let el!: HTMLDialogElement;
  const dispose = render(() => (
    <dialog ref={el} class="dlg" aria-label={title}>
      <h3>{title}</h3>
      <div class="dbody">{body()}</div>
      <div class="dacts">
        <For each={buttons}>{(b) => (
          <button class={b.primary ? "primary" : undefined} onClick={async () => { if ((await b.run?.()) !== false) handle.close(); }}>{b.label}</button>
        )}</For>
      </div>
    </dialog>
  ), host);
  const handle: DialogHandle = { el, close: () => { if (el.open) el.close(); } };
  el.addEventListener("close", () => { dispose(); host.remove(); if (openNow === handle) openNow = null; });
  openNow = handle;
  el.showModal();
  return handle;
}
