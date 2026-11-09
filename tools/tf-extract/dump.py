# Runs a Windows exe under Wine (Xvfb) and scans its memory for Tunefish 4 .tfm songs:
#   u16 I (<=32), u16 tempo, I x u16 event counts, "INST", I x 112 params, "SONG",
#   per instrument: u16 deltas[n], u8 notes[n], u8 vels[n], "ENDS".
# Saves each distinct valid song to /out/<name>-<n>.tfm. RAW=1 also dumps writable regions to /out/<name>.raw.
import os, re, struct, subprocess, sys, time, glob, hashlib
exe, name, secs = sys.argv[1], sys.argv[2], float(sys.argv[3]) if len(sys.argv) > 3 else 25

def parse(m, q):
    """(start, length) of a valid .tfm whose "INST" tag is at q, else None."""
    for n in range(1, 33):
        o = q - 4 - 2 * n
        if o < 0: break
        try:
            i, tempo = struct.unpack_from('<HH', m, o)
            if i != n or not 30 <= tempo <= 400: continue
            counts = struct.unpack_from(f'<{n}H', m, o + 4)
            p = q + 4 + 112 * n
            if m[p:p + 4] != b'SONG': continue
            p += 4 + sum(4 * c for c in counts)
            if m[p:p + 4] != b'ENDS' or sum(counts) == 0: continue
            return o, p + 4 - o
        except struct.error:
            break
    return None

def regions(pid):
    for line in open(f'/proc/{pid}/maps').read().splitlines():
        f = line.split()
        if 'r' not in f[1]: continue
        lo, hi = (int(x, 16) for x in f[0].split('-'))
        if hi - lo > 2 << 30 or lo > 0x7fffffffffff: continue
        yield lo, hi, f[1], (f[5] if len(f) > 5 else '')

def scan(pid, found):
    try:
        mem = open(f'/proc/{pid}/mem', 'rb', 0)
        rs = list(regions(pid))
    except OSError:
        return
    for lo, hi, perms, path in rs:
        CH, OV = 64 << 20, 1 << 20
        for start in range(lo, hi, CH):
            try:
                mem.seek(start); m = mem.read(min(CH + OV, hi - start))
            except (OSError, ValueError, OverflowError):
                continue
            for mt in re.finditer(rb'INST', m):
                r = parse(m, mt.start())
                if r:
                    b = m[r[0]:r[0] + r[1]]; h = hashlib.sha1(b).hexdigest()[:10]
                    if h not in found:
                        found[h] = b
                        print(f'found tfm {h}: {len(b)} bytes at {start + r[0]:#x} (instruments {b[0]}, tempo {struct.unpack_from("<H", b, 2)[0]})', flush=True)

def rawdump(pid):
    with open(f'/out/{name}.raw', 'wb') as out, open(f'/proc/{pid}/mem', 'rb', 0) as mem:
        for lo, hi, perms, path in regions(pid):
            if 'w' not in perms or path.endswith(('.so', '.dll', '.drv')) or hi - lo > 512 << 20: continue
            try:
                mem.seek(lo); d = mem.read(hi - lo)
            except (OSError, ValueError, OverflowError):
                continue
            out.write(struct.pack('<QQ', lo, len(d))); out.write(d)

env = dict(os.environ)
if open(exe, 'rb').read(4096).find(b'PE\0\0d\x86') >= 0:
    env.update(WINEARCH='win64', WINEPREFIX='/root/.wine64')
proc = subprocess.Popen(['xvfb-run', '-a', '-s', '-screen 0 1280x720x24', 'wine', exe], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
found = {}; t0 = time.time(); last = None
SKIP = (b'wineserver', b'xvfb', b'services', b'winedevice', b'explorer', b'plugplay', b'rpcss', b'svchost', b'start.exe', b'conhost')
while time.time() - t0 < secs:
    time.sleep(1)
    for pidd in glob.glob('/proc/[0-9]*'):
        try:
            cmd = open(pidd + '/cmdline', 'rb').read().replace(b'\0', b' ')
        except OSError:
            continue
        if b'.exe' in cmd.lower() and not any(s in cmd.lower() for s in SKIP):
            last = int(pidd[6:]); scan(last, found)
    if found and time.time() - t0 > 8: break
if os.environ.get('RAW') and last:
    try: rawdump(last); print('raw dump written', flush=True)
    except OSError as e: print('raw dump failed', e)
subprocess.run(['wineserver', '-k'], stderr=subprocess.DEVNULL)
proc.kill()
for i, (h, b) in enumerate(sorted(found.items(), key=lambda kv: -len(kv[1]))):
    open(f'/out/{name}-{i}.tfm', 'wb').write(b)
print(f'{name}: {len(found)} tfm(s)')
