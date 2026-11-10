import struct
def xm_samples(path):
    d=open(path,'rb').read(); hsz=struct.unpack('<I',d[60:64])[0]
    sl,rp,nch,npat,nins=struct.unpack('<5H',d[64:74]); p=60+hsz
    for _ in range(npat):
        phl,pt,nr,ps=struct.unpack('<IBHH',d[p:p+9]); p+=phl+ps
    out=[]
    for i in range(nins):
        isz=struct.unpack('<I',d[p:p+4])[0]; ns=struct.unpack('<H',d[p+27:p+29])[0]
        shs=struct.unpack('<I',d[p+29:p+33])[0] if ns else 40
        q=p+isz; hd=[]
        for s in range(ns):
            L,ls,ll,vol,fine,typ=struct.unpack('<IIIBbB',d[q:q+15]); hd.append((L,typ)); q+=shs
        for L,typ in hd:
            raw=d[q:q+L]; q+=L
            if typ&16:
                acc=0; pcm=[]
                for k in range(0,L,2):
                    acc=(acc+struct.unpack_from('<h',raw,k)[0]+32768)%65536-32768; pcm.append(acc)
                out.append((i+1,'w',pcm))
            else:
                acc=0; pcm=bytearray(L)
                for k,v in enumerate(raw): acc=(acc+v)&0xff; pcm[k]=acc
                out.append((i+1,'b',bytes(pcm)))
        p=q
    return out


def xm_replace(path, replace):
    """The XM at `path` with sample data replaced: `replace` maps a sample's index in file order (as `xm_samples`
    lists them) to its new PCM (same length; signed bytes for 8-bit, ints for 16-bit), delta-encoded as FT2 stores
    it. Headers, patterns and every other byte are kept."""
    d = bytearray(open(path, 'rb').read())
    hsz = struct.unpack('<I', d[60:64])[0]
    sl, rp, nch, npat, nins = struct.unpack('<5H', d[64:74])
    p = 60 + hsz
    for _ in range(npat):
        phl, pt, nr, ps = struct.unpack('<IBHH', d[p:p + 9]); p += phl + ps
    k = 0
    for _ in range(nins):
        isz = struct.unpack('<I', d[p:p + 4])[0]; ns = struct.unpack('<H', d[p + 27:p + 29])[0]
        shs = struct.unpack('<I', d[p + 29:p + 33])[0] if ns else 40
        q = p + isz; hd = []
        for _ in range(ns):
            L, typ = struct.unpack('<I', d[q:q + 4])[0], d[q + 14]; hd.append((L, typ)); q += shs
        for L, typ in hd:
            if k in replace:
                pcm = replace[k]
                if typ & 16:
                    assert len(pcm) * 2 == L
                    prev, out = 0, bytearray()
                    for v in pcm:
                        out += struct.pack('<h', ((v - prev + 32768) & 0xffff) - 32768); prev = v
                else:
                    assert len(pcm) == L
                    prev, out = 0, bytearray()
                    for v in pcm:
                        out.append((v - prev) & 0xff); prev = v
                d[q:q + L] = out
            q += L; k += 1
        p = q
    return bytes(d)
