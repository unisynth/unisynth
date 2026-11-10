#!/usr/bin/env python3
"""Write the generator maps (`xm-gen/1`) of the 1998 softsynth intros from their executables (change
add-plain-xm-import, design D6): unpack, run the sample calculation once (uninterrupted), record which code wrote
every byte of the module buffer and what the generators were called with, then mark each XM sample stored (written by
a copy or unpack loop) or generated (written by a generator).

    tools/xm-gen/run.sh [intro ...]       (venv with unicorn + capstone in tools/xm-gen/out/venv)
    python3 extract.py [intro ...]        intros: fudge-asc, quad-theta, quad-pure (default: all)

Material from $ZENEKK_DIR (default ~/Downloads/zenekk): the XMs and the executables (re/fdg_asc/KKLONE.EXE,
re/qd-theta/THETA.EXE, re/qd-pure/PURE.EXE, from the scene.org archives). Missing material skips with a message.
Writes maps/<intro>.json and, locally, $ZENEKK_DIR/xm-gen/<intro>.reference.xm (the XM with every generated sample
as this build generated it: the plain-XM parity's reference). maps/<intro>.json holds only the stored/generated assignment, lengths and the generators' parameters (the song's
patch data); no code, no sample data. Fudge's records are complete: the call's frame count (the XM may keep fewer),
the nine raw 32-bit arguments (float bit patterns, except the integer flags) and the drum's carried shaper value. The two Quad maps mark the generated
samples `poespack` with their generation slot only: their parameters arrive with add-poespack-softsynth.
"""
import json, os, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from emulate import Emu  # noqa: E402
from le_unupx import unpack  # noqa: E402
from xmsamples import xm_replace, xm_samples  # noqa: E402

ZENEKK = os.path.expanduser(os.environ.get('ZENEKK_DIR', '~/Downloads/zenekk'))

# Addresses are in the unpacked images (object 1 at 0x10000).
INTROS = {
    'fudge-asc': dict(
        exe='re/fdg_asc/KKLONE.EXE', xm='Fudge - Alien Sex Clone.xm',
        # the driver (0x2d10f) zeroes a buffer from its heap, copies the module template, then calls the generators
        start=0x10441, stop=0x10446, module=lambda e: e.reg(UC_EAX), offset=0, length=0x124f80, buffer=0x2000004,
        pokes={0x327e4: 0x2000000, 0x327e8: 0x1000000, 0x2000000: 0xffffffc},
        stored={0x2d13c},  # the template copy
        engines={0x2cfaa: 'fm-shaper', 0x2cdd7: 'sweep-drum'},
    ),
    'quad-theta': dict(
        exe='re/qd-theta/THETA.EXE', xm='Quad - Theta.xm',
        start=0x11b5d, stop=0x11bd6, module=lambda e: e.dword(0x5a73c) + 0x4000, offset=0x4000, length=0xf37bf, buffer=0x1000000,
        seed=(0x5a70c, 0x839b7), alloc_table=0x3a6c8,
        stored={0x11d20, 0x11d6f, 0x11dc4, 0x11e19},  # one 4-bit unpack loop per stored sample
    ),
    'quad-pure': dict(
        exe='re/qd-pure/PURE.EXE', xm='Quad - Pure.xm',
        start=0x10f3d, stop=0x10f94, module=lambda e: e.dword(0x99844) + 0x4000, offset=0x4000, length=0xfa3a0, buffer=0x1000000,
        seed=(0x9983c, 0x839b7), alloc_table=0x3bc2c, pokes={0x3bc00: 0x4800000},  # progress graphics to scratch
        stored={0x110c7, 0x110d0},  # the shared 4-bit unpack (two nibbles a byte)
    ),
}

from unicorn.x86_const import UC_X86_REG_EAX as UC_EAX, UC_X86_REG_EDX as UC_EDX, UC_X86_REG_ESP as UC_ESP  # noqa: E402


def locate(buf, samples):
    """Each XM sample's offset in the buffer: its bytes, else a 64-byte probe from its middle, else between its
    located neighbours (a sample the XM's later build changed)."""
    at = []
    for _, kind, pcm in samples:
        raw = struct.pack(f'<{len(pcm)}h', *pcm) if kind == 'w' else bytes(pcm)
        k = buf.find(raw) if raw else -1
        if k < 0 and len(raw) >= 128:
            k = buf.find(raw[len(raw) // 2:len(raw) // 2 + 64])
            k = k - len(raw) // 2 if k >= 0 else -1
        at.append((k, len(raw)))
    for i, (k, n) in enumerate(at):
        if k < 0 and n:
            prev = next((a + m for a, m in reversed(at[:i]) if a >= 0), 0)
            nxt = next((a for a, _ in at[i + 1:] if a >= 0), len(buf))
            at[i] = (prev + (nxt - prev - n) // 2, n) if nxt - prev >= n else (-1, n)
    return at


def extract(name, c):
    exe, xmp = os.path.join(ZENEKK, c['exe']), os.path.join(ZENEKK, c['xm'])
    if not (os.path.exists(exe) and os.path.exists(xmp)):
        print(f'skip  {name}: {exe} or {xmp} missing')
        return None
    img, meta = unpack(open(exe, 'rb').read())
    e = Emu(img, meta['base'])
    for a, v in c.get('pokes', {}).items():
        e.poke(a, v)
    if 'seed' in c:
        e.seed(*c['seed'])
    if 'alloc_table' in c:
        e.alloc_table(c['alloc_table'])
    e.who(c['buffer'] + c['offset'], c['length'])
    calls = {}  # buffer offset -> (engine, frames, args, carry)
    for addr, engine in c.get('engines', {}).items():
        def hook(em, engine=engine):
            esp = em.reg(UC_ESP)
            args = [em.dword(esp + 4 + 4 * k) for k in range(9)]
            # the drum's shaper value lives in a slot of its frame ([ebp-0x10] after `push ebx, ecx, esi; enter`) and
            # carries over from the previous call; the tone's register carry is set before it is read
            carry = em.dword(esp - 0x20) if engine == 'sweep-drum' else None
            calls[em.reg(UC_EAX) - c['buffer']] = (engine, em.reg(UC_EDX), args, carry)
        e.on(addr, hook)
    e.run(c['start'], c['stop'])
    module = c['module'](e)
    assert module == c['buffer'] + c['offset'], f'{name}: module at {module:#x}, expected {c["buffer"] + c["offset"]:#x}'
    buf = bytes(e.mu.mem_read(module, c['length']))
    samples = xm_samples(xmp)
    entries, slot, replace = [], 0, {}
    by_ins = {}
    for k, ((ins, kind, pcm), (at, n)) in enumerate(zip(samples, locate(buf, samples))):
        s = by_ins[ins] = by_ins.get(ins, -1) + 1
        if not pcm:
            continue
        if at < 0:
            raise SystemExit(f'{name}: instrument {ins} sample {s} not located in the module buffer')
        writers = {}
        for w in e.writers[at:at + n]:
            writers[w] = writers.get(w, 0) + 1
        top = max(writers, key=writers.get)
        if top in c['stored']:
            entries.append({'instrument': ins, 'sample': s, 'kind': 'stored'})
            continue
        region = buf[at:at + n]
        replace[k] = list(struct.unpack(f'<{n // 2}h', region)) if kind == 'w' else list(region)
        if 'engines' in c:
            engine, frames, args, carry = calls[at]
            # the generator's own count (its envelope runs over it); the XM may keep fewer (a trimmed sample)
            assert frames >= len(pcm), f'{name}: instrument {ins}: {frames} frames generated, the XM has {len(pcm)}'
            params = {'frames': frames, 'args': args}
            if carry is not None:
                params['carry'] = carry
        else:
            engine, params = 'poespack', {'slot': slot}
            slot += 1
        entries.append({'instrument': ins, 'sample': s, 'kind': 'generated', 'engine': engine, 'length': len(pcm), 'params': params})
    # the parity reference (local): the XM with every generated sample as this build generated it
    ref = os.path.join(ZENEKK, 'xm-gen', name + '.reference.xm')
    os.makedirs(os.path.dirname(ref), exist_ok=True)
    open(ref, 'wb').write(xm_replace(xmp, replace))
    changed = sum(1 for k, (_, kind, pcm) in enumerate(samples) if k in replace and replace[k] != (list(pcm) if kind == 'b' else pcm))
    print(f'{name}: reference {ref} ({changed} generated samples differ from the XM)')
    return {'format': 'xm-gen/1', 'intro': name, 'samples': entries}


def main():
    names = sys.argv[1:] or list(INTROS)
    os.makedirs(os.path.join(HERE, 'maps'), exist_ok=True)
    for name in names:
        m = extract(name, INTROS[name])
        if m is None:
            continue
        out = os.path.join(HERE, 'maps', name + '.json')
        with open(out, 'w') as f:
            f.write('{"format": "xm-gen/1", "intro": ' + json.dumps(m['intro']) + ', "samples": [\n')
            f.write(',\n'.join('  ' + json.dumps(s) for s in m['samples']) + '\n]}\n')
        st = [s['instrument'] for s in m['samples'] if s['kind'] == 'stored']
        print(f'{name}: {len(m["samples"])} samples, stored {st}, wrote {out}')


if __name__ == '__main__':
    main()
