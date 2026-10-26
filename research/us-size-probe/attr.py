# Sums the .text symbol sizes of an MSVC linker map (legacy-mangled Rust symbols) by module path: attr.py <map> [depth]
import re,sys,collections
txt=open(sys.argv[1]).read().split('\n')
syms=[];sec=None
for l in txt:
    m=re.match(r'\s*0001:([0-9a-f]{8})\s+(\S+)\s+([0-9a-f]{16})',l)
    if m: syms.append((int(m.group(1),16),m.group(2)))
m=re.search(r'0001:00000000\s+([0-9a-f]+)H\s+\.text',"\n".join(txt)); end=int(m.group(1),16)
syms=sorted(set(syms))
def path(s):
    s=s.lstrip('_')
    if not s.startswith('ZN'): return s[:30]
    i=2;parts=[]
    while i<len(s) and s[i].isdigit():
        j=i
        while s[j].isdigit(): j+=1
        n=int(s[i:j]); parts.append(s[j:j+n]); i=j+n
    parts=[p for p in parts if not re.match(r'h[0-9a-f]{16}$',p)]
    # generic impl "<impl ..>": skip
    parts=[re.sub(r'\$.*','',p) for p in parts]
    return '::'.join(parts[:int(sys.argv[2]) if len(sys.argv)>2 else 3])
agg=collections.Counter()
for k,(a,s) in enumerate(syms):
    b=syms[k+1][0] if k+1<len(syms) else end
    agg[path(s)]+=b-a
tot=sum(agg.values())
for p,n in agg.most_common(45): print(f"{n:7d} {p}")
print(tot)
