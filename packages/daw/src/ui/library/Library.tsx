// Instrument library section (change daw-solid-ts 4.2): search, one <details> group per source (entries rendered only
// while the group is open, at most LIST_CAP), inspector for the chosen entry. Same ids/classes as the old markup.
// State: ui/state/library.ts. The inspector follows the selected track (target name, whether it can load).
import { createMemo, createSignal, For, Show, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { reconcile } from "solid-js/store";
import { catalog, current, describe, filterItems, libItems, LIST_CAP, open, openGroups, query, setCurrent, setOpen, setQuery, type LibItem, type Source, visibleSources } from "../state/library.ts";

export interface LibraryActions {
  /** The selected track has an instrument (reactive). */
  hasInst(): boolean;
  /** Name of the selected track (reactive). */
  target(): string;
  /** Plays the entry (GSM sample) or C3 on the selected track; `done` when the audition note ends. */
  audition(src: Source, it: LibItem, done: () => void): void;
  load(src: Source, it: LibItem): void;
}

let tree: HTMLElement | undefined;

export function mountLibrary(section: HTMLElement, a: LibraryActions): () => void {
  section.replaceChildren();
  return render(() => <Library a={a} />, section);
}

/** Shows one source's group (search cleared) and focuses its summary. */
export function openLibrary(src = "project"): void {
  setQuery("");
  setOpen(reconcile(openGroups(catalog(), "", src)));
  tree?.querySelector<HTMLElement>(`details[data-src="${src}"] summary`)?.focus();
}

function Library(p: { a: LibraryActions }): JSX.Element {
  return <>
    <h2>Library</h2>
    <input type="search" id="lib-q" placeholder="Search instruments…" aria-label="Search the instrument library" value={query()}
      onInput={(e) => { const q = e.currentTarget.value; setQuery(q); setOpen(reconcile(openGroups(catalog(), q, null))); }} />
    <div id="lib-tree" ref={(el) => (tree = el)}><For each={visibleSources(catalog())}>{(src) => <Group src={src} />}</For></div>
    <div id="lib-insp" aria-live="polite"><Show when={current()}>{(c) => <Inspector a={p.a} src={c().src} it={c().it} />}</Show></div>
  </>;
}

function Group(p: { src: Source }): JSX.Element {
  const all = createMemo(() => libItems(catalog(), p.src.id));
  const items = createMemo(() => filterItems(all(), query()));
  return <details data-src={p.src.id} open={!!open[p.src.id]} onToggle={(e) => setOpen(p.src.id, e.currentTarget.open)}>
    <summary><span>{p.src.label}</span><span class="kind num">{query().trim() ? items().length + "/" : ""}{all().length || (catalog() ? 0 : "…")}</span></summary>
    <ul>
      <Show when={open[p.src.id]}>
        <For each={items().slice(0, LIST_CAP)}>{(it) => (
          <li><button aria-current={current()?.it === it ? "true" : undefined} onClick={() => setCurrent({ src: p.src, it })}>
            <span class="nm">{it.name}</span><span class="kind">{it.tag}</span>
          </button></li>
        )}</For>
        <Show when={items().length > LIST_CAP}><li class="dim more">+ {items().length - LIST_CAP} more · refine the search</li></Show>
      </Show>
    </ul>
  </details>;
}

function Inspector(p: { a: LibraryActions; src: Source; it: LibItem }): JSX.Element {
  const a = p.a;
  const facts = createMemo(() => describe(p.src, p.it, a.hasInst()));
  const [playing, setPlaying] = createSignal(false);
  return <>
    <div class="iname">{p.it.name}</div>
    <div class="dim">{p.src.label} · {facts().what}</div>
    <div class="lic">{p.src.lic}</div>
    <div class="iacts">
      <Show when={p.src.id === "gsm"} fallback={
        <button class="aud" aria-label={`Audition ${p.it.name}`} disabled={!a.hasInst()}
          onClick={() => { setPlaying(true); a.audition(p.src, p.it, () => setPlaying(false)); }}>{playing() ? "■ playing C3" : "▶ Audition C3"}</button>
      }>
        <button class="aud" aria-label={`Play ${p.it.name}`} onClick={() => a.audition(p.src, p.it, () => {})}>▶ Play sample</button>
      </Show>
      <button class="loadin" disabled={!facts().canLoad} onClick={() => a.load(p.src, p.it)}>Load into <span id="lib-target">{a.target()}</span></button>
    </div>
    <Show when={facts().legacy}>
      <div class="dim small">{p.src.id === "tests" ? "Test patches are listed for reference." : <>Not converted yet: run <code>cargo run --release -p synth-convert --example library</code> (writes project/music/.library.json).</>}</div>
    </Show>
    <Show when={facts().conv}><div class="dim small">{facts().conv}</div></Show>
  </>;
}
