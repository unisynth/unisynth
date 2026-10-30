"""Retrigger trace of the bit-exact references (no audio): for every rendered record, in render order, each
retrigger call's step and the compounded A/B (after `mul`, stored back) and the working v1/v2 (after the accent).

    python3 tools/retrigger_trace.py > ../../crates/synth-convert/tests/data/cauldron_retriggers.json

Step -1 is the call at the start of a variant (it reads accent[seg_reg], seg_reg left by the previous render).
"""
import json, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'ref'))
import mcauldron1 as m1, mcauldron2 as m2

def trace(mod, syn, rec, extra):
    P = dict(rec); P['raw'] = bytes.fromhex(rec['raw']); P.update(extra)
    st = dict(phase=0, phase2=0, v1=0.0, v2=0.0, q=0.0, d=0.0, y=0.0, prev=0, pitch=0)
    out = []
    def call(variant, step):
        syn.retrigger(P, st)
        out.append([variant, step, P['A'], P['B'], st['v1'], st['v2']])
    for v in range(P['variants']):
        call(v, -1)
        for s in range(16):
            syn.seg_reg = s
            if P['tie'][s] == 0:
                call(v, s)
        syn.seg_reg = 16
    return out

res = {}
j = json.load(open(os.path.join(os.path.dirname(__file__), '..', 'ref', 'patches_jizz.json')))
syn = m1.Synth.__new__(m1.Synth); syn.seg_reg = 0
res['jizz'] = [dict(record=r, calls=trace(m1, syn, j['records'][r], {})) for r in j['order']]
s = json.load(open(os.path.join(os.path.dirname(__file__), '..', 'ref', 'patches_stash.json')))
syn = m2.Synth.__new__(m2.Synth); syn.seg_reg = 0
for song in s['calls']:
    res[song['name']] = [dict(record=0 if slot == 0 else slot - 1, calls=trace(m2, syn, song['records'][0 if slot == 0 else slot - 1], {'after_tie': song['after_tie']}))
                         for slot in range(song['slots']) if slot != 1]
json.dump(res, sys.stdout)
