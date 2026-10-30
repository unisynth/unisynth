import struct, sys
def parse(path):
    b=open(path,'rb').read()
    hs=struct.unpack_from('<I',b,60)[0]
    songlen,restart,nch,npat,nins,flags,tempo,bpm=struct.unpack_from('<8H',b,64)
    order=list(b[80:80+songlen])
    p=60+hs; pats=[]
    for i in range(npat):
        phl=struct.unpack_from('<I',b,p)[0]; rows=struct.unpack_from('<H',b,p+5)[0]; psz=struct.unpack_from('<H',b,p+7)[0]
        pats.append((p+phl,rows,psz)); p+=phl+psz
    ins=[]
    for i in range(nins):
        ihs=struct.unpack_from('<I',b,p)[0]; name=b[p+4:p+26].rstrip(b'\0').decode('latin1'); ns=struct.unpack_from('<H',b,p+27)[0]
        I={'name':name,'samples':[],'off':p}
        if ns:
            I['keymap']=list(b[p+33:p+33+96])
            I['volenv']=struct.unpack_from('<24H',b,p+129); I['panenv']=struct.unpack_from('<24H',b,p+177)
            I['nvol'],I['npan'],I['vsus'],I['vls'],I['vle'],I['psus'],I['pls'],I['ple'],I['vtype'],I['ptype']=b[p+225:p+235]
            I['vibtype'],I['vibsweep'],I['vibdepth'],I['vibrate']=b[p+235:p+239]
            I['fadeout']=struct.unpack_from('<H',b,p+239)[0]
            q=p+ihs; hdrs=[]
            for j in range(ns):
                ln,ls,ll=struct.unpack_from('<3I',b,q+40*j); vol,fine,typ,pan,rel,res=struct.unpack_from('<BbBBbB',b,q+40*j+12)
                sname=b[q+40*j+18:q+40*j+40].rstrip(b'\0').decode('latin1')
                hdrs.append(dict(len=ln,ls=ls,ll=ll,vol=vol,fine=fine,type=typ,pan=pan,rel=rel,name=sname))
            d=q+40*ns
            for h in hdrs:
                h['dataoff']=d; d+=h['len']
            I['samples']=hdrs; p=d
        else: p+=ihs
        ins.append(I)
    return dict(b=b,nch=nch,npat=npat,nins=nins,tempo=tempo,bpm=bpm,order=order,flags=flags,pats=pats,ins=ins,restart=restart,end=p)
def pcm(x,h):
    raw=x['b'][h['dataoff']:h['dataoff']+h['len']]
    out=[]; acc=0
    if h['type']&16:
        for i in range(0,len(raw)-1,2):
            acc=(acc+struct.unpack_from('<h',raw,i)[0]+32768)%65536-32768; out.append(acc)
    else:
        for c in raw:
            acc=(acc+c+128)%256-128; out.append(acc*256)
    return out
if __name__=='__main__':
    x=parse(sys.argv[1])
    print(f"ch={x['nch']} pat={x['npat']} ins={x['nins']} tempo={x['tempo']} bpm={x['bpm']} flags={x['flags']} order={x['order']} end={x['end']} size={len(x['b'])}")
    for i,I in enumerate(x['ins']):
        s=' | '.join(f"len={h['len']} type={h['type']:#x} ls={h['ls']} ll={h['ll']} vol={h['vol']} fine={h['fine']} rel={h['rel']} pan={h['pan']} '{h['name']}'" for h in I['samples'])
        print(f"{i+1:2d} '{I['name']}' ns={len(I['samples'])} {s}")
