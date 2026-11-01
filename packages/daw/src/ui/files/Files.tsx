// Songs section of the browser (change daw-solid-ts 4.2): song list (current song marked, track count, • when
// unsaved), Import blob…, New song. Same ids as the old markup. State: ui/state/files.ts.
import { For, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { base, dirty, songFile, songs } from "../state/files.ts";

export interface FilesActions {
  open(path: string): void;
  importFile(file: File): void;
  newSong(): void;
  /** Track count of the open song (reactive). */
  trackCount(): number;
}

export function mountFiles(section: HTMLElement, a: FilesActions): () => void {
  section.replaceChildren();
  return render(() => <Files a={a} />, section);
}

function Files(p: { a: FilesActions }): JSX.Element {
  const a = p.a;
  let file!: HTMLInputElement;
  return <>
    <h2>Songs · project/music</h2>
    <ul id="songlist">
      <For each={songs()}>{(f) => {
        const cur = () => f === songFile();
        return <li><button aria-current={cur() ? "true" : undefined} onClick={() => a.open(f)}>
          {cur() && dirty().on ? "• " : ""}{base(f)}{cur() ? <>{" "}<span class="kind">{a.trackCount()} tr</span></> : null}
        </button></li>;
      }}</For>
    </ul>
    <ul>
      <li><button id="imp-btn" class="dim" onClick={() => file.click()}>Import blob…</button>
        <input ref={file} id="imp-file" type="file" accept=".bin,application/octet-stream" hidden
          onChange={(e) => { const f = e.currentTarget.files?.[0]; e.currentTarget.value = ""; if (f) a.importFile(f); }} /></li>
      <li><button id="new-btn" class="dim" onClick={() => a.newSong()}>New song</button></li>
    </ul>
  </>;
}
