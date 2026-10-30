import sys, collections
sys.path.insert(0,'tools'); from xminfo import parse
NOTES=['C-','C#','D-','D#','E-','F-','F#','G-','G#','A-','A#','B-']
def cells(x,p):
    off,rows,psz=x['pats'][p]; b=x['b']; i=off; end=off+psz; out=[]
    if psz==0: return out
    for r in range(rows):
        row=[]
        for c in range(x['nch']):
            v=b[i]
            if v&0x80:
                i+=1; f=[0]*5
                for k in range(5):
                    if v&(1<<k): f[k]=b[i]; i+=1
            else:
                f=list(b[i:i+5]); i+=5
            row.append(f)
        out.append(row)
    return out
x=parse(sys.argv[1])
use=collections.defaultdict(collections.Counter); rowsper=collections.defaultdict(list)
for p in range(x['npat']):
    rr=cells(x,p)
    last={}
    for r,row in enumerate(rr):
        for c,(n,ins,vol,fx,fp) in enumerate(row):
            if ins and n and n<97:
                nm=x['ins'][ins-1]['samples'][0]['name'].strip() if x['ins'][ins-1]['samples'] else '?'
                use[(ins,nm)][NOTES[(n-1)%12]+str((n-1)//12)]+=1
                if (c,ins) in last: rowsper[(ins,nm)].append(r-last[(c,ins)])
                last[(c,ins)]=r
for k in sorted(use):
    gaps=collections.Counter(rowsper[k]).most_common(3)
    print(k, dict(use[k]), 'retrigger gaps(rows)',gaps)
