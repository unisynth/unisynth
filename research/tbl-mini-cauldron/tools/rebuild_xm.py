"""Rebuild playable XMs from the XM templates + reference renders, and write
each generated phrase as a WAV at its native playback rate (R/decim).

Usage: rebuild_xm.py <out_dir>   (expects run_all.sh output)
Writes <out>/listen/{jizz,stash_song1,stash_bluish}.xm, <out>/listen/phrases/*.wav,
<out>/listen/phrases.json (per phrase: song, record, variant, decim, rate, Nyquist) and,
when ffmpeg exists, <out>/listen/ref44/<phrase>.wav (16-bit, 44.1 kHz) for the parity suite.
"""
import html, json, os, shutil, subprocess, sys, wave
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from xminfo import parse
R = 2 * 8363 * 2 ** (13 / 12)   # internal synth rate (Hz)

def delta(raw):
    out, prev = bytearray(len(raw)), 0
    for i, b in enumerate(raw):
        out[i] = (b - prev) & 0xFF; prev = b
    return bytes(out)

def wav(path, raw, rate):
    with wave.open(path, 'wb') as w:
        w.setnchannels(1); w.setsampwidth(1); w.setframerate(round(rate))
        w.writeframes(bytes((b + 128) & 0xFF for b in raw))   # WAV 8-bit is unsigned

def put(xm, h, raw):
    assert len(raw) == h['len'], (h['name'], len(raw), h['len'])
    xm[h['dataoff']:h['dataoff'] + h['len']] = delta(raw)

def main():
    out = sys.argv[1]; L = os.path.join(out, 'listen'); P = os.path.join(L, 'phrases')
    os.makedirs(P, exist_ok=True)
    phrases = []
    def phrase(name, raw, song, record, variant, decim):
        rate = R / decim
        wav(os.path.join(P, name + '.wav'), raw, rate)
        phrases.append(dict(name=name, song=song, record=record, variant=variant, decim=decim,
                            len=len(raw), rate_hz=rate, nyquist_hz=rate / 2))
    # Jizz: by name
    x = parse(os.path.join(out, 'jizz/xm_41a6e4.xm')); b = bytearray(x['b'])
    by = {h['name'].strip(): h for I in x['ins'] for h in I['samples']}
    rd = os.path.join(out, 'render_jizz')
    for r in json.load(open(os.path.join(rd, 'renders.json'))):
        raw = open(os.path.join(rd, r['name'] + '.s8'), 'rb').read()
        put(b, by[r['name'] + '.smp'], raw)
        phrase(f"jizz_{r['name']}", raw, 'jizz', r['record'], r['variant'], 2)
    open(os.path.join(L, 'jizz.xm'), 'wb').write(b)
    # Stash: by XM order, drum slot consumes drum_count samples (left as stored)
    pj = json.load(open(os.path.join(os.path.dirname(__file__), '..', 'ref', 'patches_stash.json')))
    files = {'bluish': ('stash/xm_5389b8.xm', 'stash_bluish.xm'), 'song0': ('stash/xm_45cf38.xm', 'stash_song1.xm')}
    rd = os.path.join(out, 'render_stash'); renders = json.load(open(os.path.join(rd, 'renders.json')))
    for call, song in enumerate(pj['calls']):
        src, dst = files[song['name']]
        x = parse(os.path.join(out, src)); b = bytearray(x['b'])
        smps = [h for I in x['ins'] for h in I['samples'] if h['len'] > 0]; q = 0
        for r in [r for r in renders if r['call'] == call]:
            if r['kind'] == 'drums': q += song['drum_count']; continue
            h = smps[q]; q += 1
            raw = open(os.path.join(rd, r['name'] + '.s8'), 'rb').read()
            put(b, h, raw)
            rec = 0 if r['slot'] == 0 else r['slot'] - 1
            phrase(f"stash_{r['name']}_{h['name'].strip().replace('.smp','')}", raw, 'stash_' + song['name'], rec, r['variant'],
                   song['records'][rec]['decim'] if song['records'][rec]['decim'] in (1, 2) else 4)
        open(os.path.join(L, dst), 'wb').write(b)
    json.dump(phrases, open(os.path.join(L, 'phrases.json'), 'w'), indent=1)
    ref44(L, phrases)
    write_index(L)
    print('wrote', L, len(phrases), 'phrases')

def ref44(L, phrases):
    """Each phrase resampled to 44.1 kHz 16-bit (the per-phrase parity reference)."""
    if not shutil.which('ffmpeg'):
        print('ffmpeg missing: no ref44 phrase references'); return
    d = os.path.join(L, 'ref44'); os.makedirs(d, exist_ok=True)
    for p in phrases:
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', os.path.join(L, 'phrases', p['name'] + '.wav'),
                        '-ar', '44100', '-c:a', 'pcm_s16le', os.path.join(d, p['name'] + '.wav')], check=True)

def write_index(L):
    d = os.path.join(L, 'phrases'); fs = os.listdir(d)
    ext = '.mp3' if any(f.endswith('.mp3') for f in fs) else '.wav'
    ph = sorted(f for f in fs if f.endswith(ext))
    rows = lambda pre: '\n'.join(f'<tr><td>{html.escape(p[:-4])}</td><td><audio controls preload="none" src="phrases/{p}"></audio></td></tr>'
                                for p in ph if p.startswith(pre))
    songs = [('jizz', "Jizz (Wired '97)"), ('stash_song1', 'Stash song 1 (untitled, plays first)'), ('stash_bluish', 'Stash song 2 "BLUISH"')]
    # converted by synth-convert into unisynth songs (run_all.sh's last step), for A/B against the reference
    conv = lambda f: (f'<td>unisynth: <audio controls preload="none" src="{f}.unisynth.mp3"></audio></td>'
                      if os.path.exists(os.path.join(L, f + '.unisynth.mp3')) else '')
    srows = '\n'.join(f'<tr><td>{t}</td><td>reference: <audio controls preload="none" src="{f}.mp3"></audio></td>{conv(f)}</tr>' for f, t in songs)
    open(os.path.join(L, 'index.html'), 'w').write(f'''<!doctype html><meta charset=utf-8><title>mini-cauldron renders</title>
<style>body{{font:14px system-ui;margin:2em;max-width:60em}}td{{padding:2px 10px}}audio{{height:28px}}</style>
<h1>mini-cauldron: rendered by our reference</h1>
<p>Songs: original XM templates with every generated sample replaced by the Python reference output
(byte-identical to the intro's), played by libopenmpt at 44.1 kHz. Phrases: single 16-step renders at native rate.</p>
<h2>Full songs</h2><table>{srows}</table>
<h2>Jizz phrases (v1)</h2><table>{rows('jizz_')}</table>
<h2>Stash song 1 phrases (v2, call 1)</h2><table>{rows('stash_song0')}</table>
<h2>Stash BLUISH phrases (v2, call 0)</h2><table>{rows('stash_bluish')}</table>
''')

if __name__ == '__main__':
    main()
