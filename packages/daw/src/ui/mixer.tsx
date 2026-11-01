// Mixer panel in SolidJS (change daw-solid-ts). Strips read the song store directly (`state/song.ts`: app.js writes
// through the store proxies, so a fader, mute or send write updates just its control) and the meters from
// `state/live.ts`. Edits go through `MixerActions` (app.js: model, undo, livePush). Mounted only while the Mixer
// tab shows (`setMixerShown`): a hidden mixer has no DOM to update.
import { createSignal, For, Show, type JSX } from "solid-js";
import { render } from "solid-js/web";
import type { UITrack } from "../model.ts";
import { peaks } from "./state/live.ts";
import type { SongRoot } from "./state/song.ts";
import { selectedId } from "./state/view.ts";
import { shown, type MixerActions } from "./mixer-store.ts";

export { setMixerShown } from "./mixer-store.ts";

export function mountMixer(el: HTMLElement, song: SongRoot, actions: MixerActions): () => void {
  el.replaceChildren();
  return render(() => <Show when={shown()}><Mixer song={song} a={actions} /></Show>, el);
}

function Mixer(p: { song: SongRoot; a: MixerActions }): JSX.Element {
  return <>
    <For each={p.song.tracks}>{(t) => <Strip t={t} a={p.a} />}</For>
    <button class="addbus" title="Add a bus track: no instrument, fed by sends" onClick={() => p.a.addBus()}>+ Bus</button>
    <Strip t={p.song.master} a={p.a} />
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

function Strip(p: { t: UITrack; a: MixerActions }): JSX.Element {
  const t = p.t, a = p.a;
  return <div role="group" aria-label={`${t.name} channel strip`}
    class={"strip" + (t.master ? " master" : "") + (t.bus ? " bus" : "") + (selectedId() === t.id ? " sel" : "")}>
    <button class="sn" aria-label={`Select ${t.name}`} onClick={() => a.select(t.id)}><i style={{ background: `var(${t.col})` }} />{t.name}</button>
    <div class="sec"><div class="sh">Inserts</div>
      <ul class="inslist">
        <For each={t.devices}>{(d, i) =>
          <Show when={d.kind !== 64}>
            <li class={d.bypass ? "off" : undefined}>
              <button class="ins" title={`Open ${a.devName(d)} in the device panel`} onClick={() => a.openInsert(t.id)}>{a.devName(d)}</button>
              <button class="byp" aria-pressed={!!d.bypass} aria-label={`Bypass ${a.devName(d)} on ${t.name}`} title="Bypass" onClick={() => a.bypass(t.id, i())}>⏻</button>
            </li>
          </Show>}
        </For>
        <Show when={t.devices.every((d) => d.kind === 64)}><li class="dim none">none</li></Show>
      </ul>
      <Picker label={`Effect to insert on ${t.name}`} button="addins" buttonLabel={`Add insert effect to ${t.name}`} empty="choose effect…"
        options={() => a.effects().map((f) => [f, f])} pick={(f) => a.addInsert(t.id, f)} />
    </div>
    <Show when={!t.master}>
      <div class="sec"><div class="sh">Sends <span class="dim">post</span></div>
        <ul class="sendlist">
          <For each={t.sends} fallback={<li class="dim none">none</li>}>{(sd, i) => {
            let gesture = false;
            return <li>
              <span>→ {sd[0]}<Show when={sd[1] === "side"}> <em>side</em></Show></span>
              <input class="sendlvl" type="range" min="-60" max="6" step="0.1" value={sd[2]}
                aria-label={`Send ${t.name} to ${sd[0]} ${sd[1] === "side" ? "side" : "main"} level`} aria-valuetext={`${dB(sd[2])} dB`}
                onInput={(e) => { gesture = true; a.sendLevel(t.id, i(), +e.currentTarget.value, false); }}
                onChange={(e) => { if (gesture) { gesture = false; a.sendLevel(t.id, i(), +e.currentTarget.value, true); } }} />
              <span class="num">{dB(sd[2])}</span>
              <button class="srm" aria-label={`Remove send ${t.name} to ${sd[0]}`} title="Remove send" onClick={() => a.removeSend(t.id, i())}>✕</button>
            </li>;
          }}</For>
        </ul>
        <Picker label={`Send target for ${t.name}`} button="addsend" buttonLabel={`Add send from ${t.name}`} empty="send to…"
          options={() => a.sendTargets(t.id).map(([n, ch]) => [`${n}|${ch}`, `${n} · ${ch === "side" ? "sidechain" : "main"}`])}
          pick={(v) => { const [to, ch] = v.split("|"); a.addSend(t.id, to, ch as "main" | "side"); }} />
      </div>
    </Show>
    <div class="fm">
      <input class="fader" type="range" min="-60" max="6" step="0.1" value={t.vol} aria-label={`${t.name} volume`} aria-valuetext={`${dB(t.vol)} dB`}
        onInput={(e) => a.volume(t.id, +e.currentTarget.value, false)} onChange={(e) => a.volume(t.id, +e.currentTarget.value, true)} />
      <Meter id={t.id} name={t.name} c={0} />
      <Meter id={t.id} name={t.name} c={1} />
    </div>
    <div class="db num">{dB(t.vol)} dB</div>
    <Show when={!t.master} fallback={<div class="dim" style={{ "font-size": "10px" }}>→ song output</div>}>
      <div class="ms"><button aria-pressed={!!t.mute} aria-label={`Mute ${t.name}`} onClick={() => a.mute(t.id)}>M</button><button aria-pressed={!!t.solo} aria-label={`Solo ${t.name}`} onClick={() => a.solo(t.id)}>S</button></div>
    </Show>
  </div>;
}
