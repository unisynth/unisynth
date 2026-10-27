# Runs a Windows exe under Wine (Xvfb) and scans its memory for WaveSabre serialized songs
# (Docs/Serialized-song-format.md). Saves each distinct valid blob to /out/<name>-<n>.bin.
import os, re, struct, subprocess, sys, time, glob, hashlib
exe, name, secs = sys.argv[1], sys.argv[2], float(sys.argv[3]) if len(sys.argv) > 3 else 25

def parse(m, o):
    """Length of a valid song blob at o, else None."""
    try:
        tempo, rate = struct.unpack_from('<ii', m, o)
        length, = struct.unpack_from('<d', m, o + 8)
        if not (20 <= tempo <= 999 and rate == 44100 and 0.5 < length < 7200): return None
        p = o + 16
        nd, = struct.unpack_from('<i', m, p); p += 4
        if not 0 < nd < 512: return None
        for _ in range(nd):
            did = m[p]; size, = struct.unpack_from('<i', m, p + 1); p += 5
            if did > 40 or not 0 <= size < 4_000_000: return None
            p += size
        nl, = struct.unpack_from('<i', m, p); p += 4
        if not 0 <= nl < 512: return None
        for _ in range(nl):
            ne, = struct.unpack_from('<i', m, p); p += 4
            if not 0 <= ne < 2_000_000: return None
            for _ in range(ne):
                dt, = struct.unpack_from('<i', m, p); b = m[p + 4]; p += 5
                if dt < 0: return None
                if not b & 0x80: p += 1
        nt, = struct.unpack_from('<i', m, p); p += 4
        if not 0 < nt < 512: return None
        for _ in range(nt):
            vol, = struct.unpack_from('<f', m, p); p += 4
            if not 0 <= vol < 100: return None
            nr, = struct.unpack_from('<i', m, p); p += 4
            if not 0 <= nr < 512: return None
            p += 12 * nr
            ndv, = struct.unpack_from('<i', m, p); p += 4
            if not 0 <= ndv < 512: return None
            p += 4 * ndv
            lane, na = struct.unpack_from('<ii', m, p); p += 8
            if not (-1 <= lane < max(nl, 1) and 0 <= na < 4096): return None
            for _ in range(na):
                di, pid, npt = struct.unpack_from('<iii', m, p); p += 12
                if not (0 <= di < nd and 0 <= pid < 1024 and 0 <= npt < 2_000_000): return None
                p += 5 * npt
        return p - o if p <= len(m) else None
    except (struct.error, IndexError):
        return None

def scan(pid, found):
    try:
        maps = open(f'/proc/{pid}/maps').read().splitlines()
        mem = open(f'/proc/{pid}/mem', 'rb', 0)
    except OSError:
        return
    for line in maps:
        a, perms = line.split()[:2]
        if 'r' not in perms: continue
        lo, hi = (int(x, 16) for x in a.split('-'))
        if hi - lo > 2 << 30 or lo > 0x7fffffffffff: continue
        CH, OV = 64 << 20, 4 << 20   # chunks with overlap: a blob is at most a few MB
        for start in range(lo, hi, CH):
            try:
                mem.seek(start); m = mem.read(min(CH + OV, hi - start))
            except (OSError, ValueError, OverflowError):
                continue
            for mt in re.finditer(rb'\x44\xac\x00\x00', m):
                o = mt.start() - 4
                if o < 0: continue
                n = parse(m, o)
                if n:
                    b = m[o:o + n]; h = hashlib.sha1(b).hexdigest()[:10]
                    if h not in found:
                        found[h] = b
                        print(f'found blob {h}: {n} bytes at {start + o:#x} (tempo {struct.unpack_from("<i", b)[0]}, {struct.unpack_from("<d", b, 8)[0]:.1f} s, {struct.unpack_from("<i", b, 16)[0]} devices)', flush=True)

env = dict(os.environ)
if open(exe, 'rb').read(4096).find(b'PE\0\0d\x86') >= 0:
    env.update(WINEARCH='win64', WINEPREFIX='/root/.wine64')
proc = subprocess.Popen(['xvfb-run', '-a', '-s', '-screen 0 1280x720x24', 'wine', exe], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
found = {}; t0 = time.time()
while time.time() - t0 < secs:
    time.sleep(0.5)
    for pidd in glob.glob('/proc/[0-9]*'):
        try:
            cmd = open(pidd + '/cmdline', 'rb').read().replace(b'\0', b' ')
        except OSError:
            continue
        if b'.exe' in cmd.lower() and b'wineserver' not in cmd and b'xvfb' not in cmd.lower() and b'services' not in cmd and b'winedevice' not in cmd and b'explorer' not in cmd and b'plugplay' not in cmd and b'rpcss' not in cmd and b'svchost' not in cmd:
            scan(int(pidd[6:]), found)
    if found and time.time() - t0 > 6: break
subprocess.run(['wineserver', '-k'], stderr=subprocess.DEVNULL)
proc.kill()
for i, (h, b) in enumerate(sorted(found.items(), key=lambda kv: -len(kv[1]))):
    open(f'/out/{name}-{i}.bin', 'wb').write(b)
print(f'{name}: {len(found)} blob(s)')
