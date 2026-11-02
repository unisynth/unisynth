// File dialogs (change daw-solid-ts 4.2): save prompt, validation errors, export summary, bounce. Built on
// ui/common/Dialog.tsx; ui/app/files.ts supplies the facts and the actions (engine, files). Tables need an explicit <tbody>:
// the HTML parser inserts one into Solid's template and the compiled node walk then misses.
import { createSignal, For, Match, Switch } from "solid-js";
import { openDialog } from "../common/Dialog.tsx";
import { base } from "../state/files.ts";

export interface PathError { path?: string; message: string }

/** "Save changes to X?": Save (stays open if saving fails), Don't save, Cancel. `go` continues the switch. */
export function confirmSave(file: string, why: string, save: () => Promise<boolean>, go: () => void): void {
  openDialog(`Save changes to ${base(file)}?`, () => <p>{why}</p>, [
    { label: "Save", primary: true, run: async () => { if (await save()) go(); else return false; } },
    { label: "Don't save", run: go },
    { label: "Cancel" },
  ]);
}

/** Validation errors with their JSON paths; clicking one closes the dialog and calls `pick(path)`. */
export function showErrors(errs: PathError[], what: string, pick: (path: string) => void): void {
  const d = openDialog(`${what} blocked: ${errs.length} validation error${errs.length > 1 ? "s" : ""}`, () => <>
    <ul class="errs"><For each={errs}>{(e) => (
      <li><button onClick={() => { d.close(); pick(e.path || "/"); }}><code>{e.path || "/"}</code> {e.message}</button></li>
    )}</For></ul>
    <p class="dim">Fix them and {what.toLowerCase()} again. Click an error to go to it.</p>
  </>, [{ label: "Close", primary: true }]);
}

/** A message and Close. */
export function notice(title: string, text: string): void {
  openDialog(title, () => <p>{text}</p>, [{ label: "Close", primary: true }]);
}

export interface ExportFacts { bin: string; kb: string; tracks: number; devices: number; notes: number; lanes: number; length: string }

export function exportDialog(f: ExportFacts, write: () => Promise<void>, download: () => void): void {
  openDialog(`Export ${base(f.bin)}`, () => (
    <table class="kv"><tbody>
      <tr><th>File</th><td><code>{f.bin}</code></td></tr>
      <tr><th>Size</th><td>{f.kb} kB before packing</td></tr>
      <tr><th>Tracks / devices</th><td>{f.tracks} + master · {f.devices} devices (kinds 64–73)</td></tr>
      <tr><th>Notes</th><td>≥ {f.notes} (linked and looped clips flattened into one lane per track; muted tracks exported silent)</td></tr>
      <tr><th>Automation</th><td>{f.lanes} lanes, quantized to 256 steps, applied per 100-frame block</td></tr>
      <tr><th>Length</th><td>{f.length}</td></tr>
      <tr><th>Validation</th><td class="ok">passed</td></tr>
    </tbody></table>
  ), [
    { label: "Write blob", primary: true, run: write },
    { label: "Download song.json", run: () => { download(); return false; } },
    { label: "Close" },
  ]);
}

export interface BounceResult { name: string; url: string; seconds: number; mb: number }
export interface BounceJob { done: Promise<BounceResult>; cancel(): void }

/** Bounce to WAV: range choice, Render (progress, then a download link), Cancel (stops a render, else closes).
 *  `start` returns null when the song cannot be exported (it reports why). */
export function bounceDialog(ranges: { song: string; loop: string }, start: (loop: boolean, progress: (v: number) => void) => BounceJob | null): void {
  const [loop, setLoop] = createSignal(false);
  const [progress, setProgress] = createSignal<number | null>(null);
  const [res, setRes] = createSignal<string | BounceResult>("");
  let job: BounceJob | null = null;
  const render = () => {
    if (job) return false;
    const j = start(loop(), (v) => { setProgress(v); setRes(`rendering… ${Math.round(v * 100)}%`); });
    if (!j) return;
    job = j; setProgress(0); setRes("rendering…");
    j.done.then((r) => { job = null; setProgress(1); setRes(r); })
      .catch((e) => { job = null; if (e.message !== "cancelled") setRes(`bounce failed: ${e.message}`); });
    return false;
  };
  const cancel = () => {
    if (!job) return;
    job.cancel(); job = null; setRes("cancelled"); setProgress(null);
    return false;
  };
  openDialog("Bounce to WAV", () => <>
    <fieldset><legend>Range</legend>
      <label><input type="radio" name="rng" value="song" checked={!loop()} onChange={() => setLoop(false)} /> Whole song <span class="dim">{ranges.song}</span></label>
      <label><input type="radio" name="rng" value="loop" checked={loop()} onChange={() => setLoop(true)} /> Loop <span class="dim">{ranges.loop}</span></label>
    </fieldset>
    <p class="dim small">44.1 kHz · 16-bit stereo · the exported blob through the intro's song player · mute applies, solo does not.</p>
    <progress max="1" value={progress() ?? 0} hidden={progress() === null} />
    <div class="bres" aria-live="polite">
      <Switch>
        <Match when={typeof res() === "string"}>{res() as string}</Match>
        <Match when={typeof res() === "object" && (res() as BounceResult)}>{(r) => <>
          Done: <a download={r().name} href={r().url}>{r().name}</a> · {r().seconds.toFixed(1)} s · {r().mb.toFixed(1)} MB
        </>}</Match>
      </Switch>
    </div>
  </>, [{ label: "Render", primary: true, run: render }, { label: "Cancel", run: cancel }]);
}

