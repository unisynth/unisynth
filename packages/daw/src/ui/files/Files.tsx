// Songs section of the browser (change daw-solid-ts 4.2): search, song list (title, length, composer as in the songs
// page player; current song marked, track count, • when unsaved), Import blob…, New song. State: ui/state/files.ts.
import { createMemo, For, Show, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { composer } from "../../model.ts";
import { base, dirty, fmtLen, infos, query, setQuery, songFile, songs } from "../state/files.ts";

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
  const shown = createMemo(() => {
    const q = query().trim().toLowerCase();
    if (!q) return songs();
    return songs().filter((f) => { const i = infos()[f]; return `${i?.title ?? ""} ${i?.author ?? ""} ${f}`.toLowerCase().includes(q); });
  });
  return <>
    <h2>Songs · project/music</h2>
    <input id="song-q" class="search" type="search" placeholder="Search title, composer, file…" aria-label="Search songs" autocomplete="off"
      value={query()} onInput={(e) => setQuery(e.currentTarget.value)} onKeyDown={(e) => { if (e.key === "Escape" && query()) { e.stopPropagation(); setQuery(""); } }} />
    <ul id="songlist">
      <For each={shown()}>{(f) => {
        const cur = () => f === songFile(), i = () => infos()[f];
        return <li><button data-file={f} title={i()?.author ? `${i()!.author}\n${f}` : f} aria-current={cur() ? "true" : undefined} onClick={() => a.open(f)}>
          <span class="tt">{cur() && dirty().on ? "• " : ""}{i()?.title || base(f).replace(/\.song\.json$/, "")}</span>
          <span class="du num">{i() ? fmtLen(i()!.seconds) : ""}</span>
          <span class="by">{i()?.author ? composer(i()!.author!) : base(f)}</span>
          {cur() ? <span class="kind">{a.trackCount()} tr</span> : null}
        </button></li>;
      }}</For>
    </ul>
    <Show when={!shown().length}><p id="songs-none" class="dim">No song matches.</p></Show>
    <ul>
      <li><button id="imp-btn" class="dim" onClick={() => file.click()}>Import blob…</button>
        <input ref={file} id="imp-file" type="file" accept=".bin,application/octet-stream" hidden
          onChange={(e) => { const f = e.currentTarget.files?.[0]; e.currentTarget.value = ""; if (f) a.importFile(f); }} /></li>
      <li><button id="new-btn" class="dim" onClick={() => a.newSong()}>New song</button></li>
    </ul>
  </>;
}
