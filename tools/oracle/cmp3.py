# cmp3.py orig remake [skip_ticks]: first tick whose event list differs
import sys, re
def parse(path, orig):
    ev={}; t=-1; pos={}
    for l in open(path):
        k=l[:2]
        if k=='T ':
            m=re.match(r'T (\d+) \(([-\d.]+),([-\d.]+)\) score=(\d+)',l); t=int(m[1]); pos[t]=(float(m[2]),float(m[3]),int(m[4])); continue
        if k=='R ':
            p=l.split(); key='R'; full=l.strip()
        elif k=='C ':
            m=re.match(r"C (\S+) .*?force=(\d) name=(['\"])(.*?)\3",l)
            key=f"C {m[1]} f{m[2]} {m[4][:24]}"; full=l.strip()
        elif k=='S ':
            key='S '+l.split()[1]; full=l.strip()
        else: continue
        ev.setdefault(t,[]).append((key,full))
    return ev,pos
oe,op=parse(sys.argv[1],True); re_,rp=parse(sys.argv[2],False)
last=min(max(op),max(rp))
for t in range(-1,last+1):
    a=[k for k,_ in oe.get(t,[])]; b=[k for k,_ in re_.get(t,[])]
    if a!=b:
        print(f"tick {t} differs")
        for u in (t-1,t):
            print(f"--- tick {u} orig pos {op.get(u)} remake pos {rp.get(u)}")
            A=oe.get(u,[]); B=re_.get(u,[])
            for i in range(max(len(A),len(B))):
                x=A[i][1] if i<len(A) else ''; y=B[i][1] if i<len(B) else ''
                mark=' ' if (i<len(A) and i<len(B) and A[i][0]==B[i][0]) else '*'
                print(f" {mark} {x[:70]:70s} | {y[:70]}")
        break
else: print("events agree through tick",last)
