// Mixer panel in SolidJS (change daw-solid-ts, spike). The song model stays in app.js; app.js pushes a plain view
// of every strip (`syncMixer`); meter levels come from `state/live.ts`; Solid diffs them (`reconcile`, keyed by track id)
// and touches only the DOM that changed. Edits go back through `MixerActions` (app.js: model, undo, livePush).
// The panel is mounted only while the Mixer tab shows (`setMixerShown`): a hidden mixer has no DOM to update.
// State lives in mixer-store.ts (survives a hot reload of this file).
import { createSignal, For, Show, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { peaks } from "./state/live.ts";
import { shown, view, type MixerActions, type StripView } from "./mixer-store.ts";

export { patchSend, patchStrip, setMixerShown, syncMixer } from "./mixer-store.ts";

export function mountMixer(el: HTMLElement, actions: MixerActions): () => void {
  el.replaceChildren();
  return render(() => <Show when={shown()}><Mixer a={actions} /></Show>, el);
}

function Mixer(p: { a: MixerActions }): JSX.Element {
  return <>
    <For each={view.tracks}>{(s) => <Strip s={s} a={p.a} />}</For>
    <button class="addbus" title="Add a bus track: no instrument, fed by sends" onClick={() => p.a.addBus()}>+ Bus</button>
    <Show when={view.master}>{(m) => <Strip s={m()} a={p.a} />}</Show>
  </>;
}

/** A button that turns into a select while choosing; blur without a choice brings the button back. */
function Picker(p: { label: string; button: string; buttonLabel: string; empty: string; options: () => [string, string][]; pick: (v: string) => void }): JSX.Element {
  const [open, setOpen] = createSignal(false);
  return <Show when={open()} fallback={<button class={p.button} aria-label={p.buttonLabel} onClick={() => setOpen(true)}>{p.button === "addins" ? "+ insert" : "+ send"}</button>}>
    {(() => {
      const opts = p.options();
      return <select aria-label={p.label} ref={(el) => queueMicrotask(() => el.focus())}
        onChange={(e) => { const v = e.currentTarget.value; if (v) { setOpen(false); p.pick(v); } }}
        onBlur={(e) => { if (!e.currentTarget.value) setOpen(false); }}>
        <option value="">{opts.length ? p.empty : "no target: every track already feeds this one"}</option>
        <For each={opts}>{([v, text]) => <option value={v}>{text}</option>}</For>
      </select>;
    })()}
  </Show>;
}

const dB = (v: number) => v.toFixed(1);

function Meter(p: { id: string; name: string; c: 0 | 1 }): JSX.Element {
  const v = () => peaks[p.id]?.[p.c] ?? 0;
  return <div class="meter" role="meter" aria-label={`${p.name} level ${p.c ? "right" : "left"}`}
    aria-valuenow={v().toFixed(2)} aria-valuemin="0" aria-valuemax="1"><i style={{ height: `${(1 - v()) * 100}%` }} /></div>;
}

function Strip(p: { s: StripView; a: MixerActions }): JSX.Element {
  const s = p.s, a = p.a;
  return <div role="group" aria-label={`${s.name} channel strip`}
    class={"strip" + (s.master ? " master" : "") + (s.bus ? " bus" : "") + (s.sel ? " sel" : "")}>
    <button class="sn" aria-label={`Select ${s.name}`} onClick={() => a.select(s.id)}><i style={{ background: `var(${s.col})` }} />{s.name}</button>
    <div class="sec"><div class="sh">Inserts</div>
      <ul class="inslist">
        <For each={s.inserts} fallback={<li class="dim none">none</li>}>{(d) =>
          <li class={d.bypass ? "off" : undefined}>
            <button class="ins" title={`Open ${d.name} in the device panel`} onClick={() => a.openInsert(s.id)}>{d.name}</button>
            <button class="byp" aria-pressed={d.bypass} aria-label={`Bypass ${d.name} on ${s.name}`} title="Bypass" onClick={() => a.bypass(s.id, d.i)}>⏻</button>
          </li>}
        </For>
      </ul>
      <Picker label={`Effect to insert on ${s.name}`} button="addins" buttonLabel={`Add insert effect to ${s.name}`} empty="choose effect…"
        options={() => a.effects().map((f) => [f, f])} pick={(f) => a.addInsert(s.id, f)} />
    </div>
    <Show when={!s.master}>
      <div class="sec"><div class="sh">Sends <span class="dim">post</span></div>
        <ul class="sendlist">
          <For each={s.sends} fallback={<li class="dim none">none</li>}>{(sd) => {
            let gesture = false;
            return <li>
              <span>→ {sd.to}<Show when={sd.side}> <em>side</em></Show></span>
              <input class="sendlvl" type="range" min="-60" max="6" step="0.1" value={sd.db}
                aria-label={`Send ${s.name} to ${sd.to} ${sd.side ? "side" : "main"} level`} aria-valuetext={`${dB(sd.db)} dB`}
                onInput={(e) => { gesture = true; a.sendLevel(s.id, sd.i, +e.currentTarget.value, false); }}
                onChange={(e) => { if (gesture) { gesture = false; a.sendLevel(s.id, sd.i, +e.currentTarget.value, true); } }} />
              <span class="num">{dB(sd.db)}</span>
              <button class="srm" aria-label={`Remove send ${s.name} to ${sd.to}`} title="Remove send" onClick={() => a.removeSend(s.id, sd.i)}>✕</button>
            </li>;
          }}</For>
        </ul>
        <Picker label={`Send target for ${s.name}`} button="addsend" buttonLabel={`Add send from ${s.name}`} empty="send to…"
          options={() => a.sendTargets(s.id).map(([n, ch]) => [`${n}|${ch}`, `${n} · ${ch === "side" ? "sidechain" : "main"}`])}
          pick={(v) => { const [to, ch] = v.split("|"); a.addSend(s.id, to, ch as "main" | "side"); }} />
      </div>
    </Show>
    <div class="fm">
      {/* volume edits go through app.js's document-level " volume" handler (keeps the header fader in sync) */}
      <input class="fader" type="range" min="-60" max="6" step="0.1" value={s.vol} aria-label={`${s.name} volume`} aria-valuetext={`${dB(s.vol)} dB`} />
      <Meter id={s.id} name={s.name} c={0} />
      <Meter id={s.id} name={s.name} c={1} />
    </div>
    <div class="db num">{dB(s.vol)} dB</div>
    <Show when={!s.master} fallback={<div class="dim" style={{ "font-size": "10px" }}>→ song output</div>}>
      {/* mute/solo go through app.js's document-level handler (one state for header + strip) */}
      <div class="ms"><button aria-pressed={s.mute} aria-label={`Mute ${s.name}`}>M</button><button aria-pressed={s.solo} aria-label={`Solo ${s.name}`}>S</button></div>
    </Show>
  </div>;
}
