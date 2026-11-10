"""cmpog.py ORIG_TRACE OG_OUTPUT: compare the port's --og-film output with a
trace3.py trace of the original, tick by tick: player 0 position, score,
state, and the rand() states drawn since the previous tick. Prints the
first difference with context."""
import re, sys
def lcg(s): return (s*1103515245+12345)&0xffffffff
def parse(path, orig):
    ticks=[]; cur=None; pre=[]
    for l in open(path):
        if l.startswith('T '):
            m=re.match(r'T (\d+) \(([-\d.]+),([-\d.]+)\) score=(\d+) st=(-?\d+)',l)
            ticks.append((int(m[1]),m[2],m[3],int(m[4]),int(m[5]),pre)); pre=[]
        elif l.startswith('R '):
            p=l.split()
            pre.append(lcg(int(p[3],16)) if orig else int(p[1],16))
    return ticks, pre
o,_=parse(sys.argv[1],True); r,_=parse(sys.argv[2],False)
od={t[0]:t for t in o}
for t in r:
    a=od.get(t[0])
    if a is None: continue
    if a[1:5]!=t[1:5] or a[5]!=t[5]:
        print("first difference at tick",t[0])
        print(" orig :",a[:5],"draws",len(a[5]),[f"{x:08x}" for x in a[5][:8]])
        print(" port :",t[:5],"draws",len(t[5]),[f"{x:08x}" for x in t[5][:8]])
        sys.exit(1)
print("agree through tick", r[-1][0] if r else None)
