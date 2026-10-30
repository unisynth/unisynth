"""Compare reference renders against the samples the emulated intro wrote into the XMs.

Usage:
  verify.py jizz  <xm>                      <render_dir>
  verify.py stash <bluish_xm> <song0_xm>    <render_dir> <drums.json>
Generated samples are matched in XM order (non-empty samples; the drum slot
consumes drum_count entries).  Exit code 1 on any mismatch.
"""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from xminfo import parse, pcm

def s8file(path):
    return [(b - 256 if b > 127 else b) for b in open(path, 'rb').read()]

def cmp(label, ref, mine):
    diff = [i for i in range(min(len(ref), len(mine))) if ref[i] != mine[i]]
    good = not diff and len(ref) == len(mine)
    print(f"{label:34s} len {len(mine)}/{len(ref)} " + ('OK' if good else f'DIFF n={len(diff)} first={diff[:4]}'))
    return good

def main():
    mode = sys.argv[1]; ok = bad = 0
    if mode == 'jizz':
        x = parse(sys.argv[2]); od = sys.argv[3]
        by = {h['name'].strip(): h for I in x['ins'] for h in I['samples']}
        for r in json.load(open(os.path.join(od, 'renders.json'))):
            h = by[r['name'] + '.smp']
            g = cmp(r['name'] + ' -> ' + r['name'] + '.smp', [v >> 8 for v in pcm(x, h)], s8file(os.path.join(od, r['name'] + '.s8')))
            ok += g; bad += not g
    else:
        xms = {'bluish': parse(sys.argv[2]), 'song0': parse(sys.argv[3])}
        od = sys.argv[4]; drums = json.load(open(sys.argv[5]))
        pj = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'ref', 'patches_stash.json')))
        renders = json.load(open(os.path.join(od, 'renders.json')))
        for call, song in enumerate(pj['calls']):
            x = xms[song['name']]
            smps = [h for I in x['ins'] for h in I['samples'] if h['len'] > 0]
            q = 0
            for r in [r for r in renders if r['call'] == call]:
                if r['kind'] == 'drums':
                    for d in drums[:song['drum_count']]:
                        h = smps[q]; q += 1
                        g = x['b'][h['dataoff']:h['dataoff'] + h['len']].hex() == d['data_hex']
                        print(f"{song['name']} stored drum len={d['len']:<5d}            {'OK' if g else 'DIFF'}")
                        ok += g; bad += not g
                    continue
                h = smps[q]; q += 1
                g = cmp(f"{r['name']} -> {h['name'].strip()}", [v >> 8 for v in pcm(x, h)], s8file(os.path.join(od, r['name'] + '.s8')))
                ok += g; bad += not g
    print('ok', ok, 'bad', bad)
    sys.exit(1 if bad else 0)

if __name__ == '__main__':
    main()
