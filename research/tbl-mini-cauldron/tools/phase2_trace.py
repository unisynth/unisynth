"""The second saw's phase at every retrigger of the bit-exact references (full renders, render order):
    python3 tools/phase2_trace.py > ../../crates/synth-convert/tests/data/cauldron_phase2.json
{call: [{record, calls: [[variant, step, phase2], ...]}]}; step -1 is a variant's first retrigger (before step 0's).
phase2 is the 16-bit word as the reference holds it.
"""
import json, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'ref'))
import mcauldron1 as m1, mcauldron2 as m2
here = os.path.dirname(os.path.abspath(__file__))

def trace(mod, syn, records, log):
    """Hooks `syn`: a new per-render state starts a log entry; a variant's first retrigger is step -1."""
    state = {'id': None, 'first': True}
    raws = [r['raw'] for r in records]
    orig_r, orig_s = mod.Synth.retrigger, mod.Synth.segments
    def retrigger(self, P, st):
        if id(st) != state['id']:
            state['id'] = id(st)
            log.append(dict(record=raws.index(P['raw'].hex()), calls=[], v=0))
            state['first'] = True
        e = log[-1]
        e['calls'].append([e['v'], -1 if state['first'] else self.seg_reg, st['phase2'] & 0xFFFF])
        state['first'] = False
        orig_r(self, P, st)
    def segments(self, P, st):
        orig_s(self, P, st)
        log[-1]['v'] += 1
        state['first'] = True
    syn.retrigger = retrigger.__get__(syn)
    syn.segments = segments.__get__(syn)

res = {}
j = json.load(open(os.path.join(here, '..', 'ref', 'patches_jizz.json')))
syn = m1.Synth(j['rng_state'])
res['jizz'] = []
trace(m1, syn, j['records'], res['jizz'])
for r in j['order']:
    syn.render_patch(j['records'][r], r)
s = json.load(open(os.path.join(here, '..', 'ref', 'patches_stash.json')))
syn = m2.Synth()
for song in s['calls']:
    res[song['name']] = []
    trace(m2, syn, song['records'], res[song['name']])
    syn.render_song(song)
for k in res:
    for e in res[k]:
        e.pop('v')
json.dump(res, sys.stdout)
