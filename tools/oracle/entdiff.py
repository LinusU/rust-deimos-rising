"""entdiff.py ORIG PORT [IGNORE_OFFSETS]: compare raw entity dumps
(entraw.py vs OG_ENTS) tick by tick; prints the first tick whose entity
lists or bytes differ, as dword offsets with both values. Offsets holding
pointers (which never match) are skipped: 0x4e, 0x8a, 0x138, and the
spawn-info list pointers 0x194..0x1e3."""
import sys, struct, collections
PTR={0x4e,0x8a,0x138}|set(range(0x194,0x1e4,4))
def load(p):
    d=collections.defaultdict(list)
    for l in open(p):
        if not l.startswith('E '): continue
        _,t,gi,ci,*rest=l.split(' ')
        unit=' '.join(rest[:-1]) if len(rest)>2 else rest[0]
        d[int(t)].append((int(gi),int(ci),l[-(0x1e4*2+1):-1]))
    return d
o=load(sys.argv[1]); r=load(sys.argv[2])
for t in sorted(set(o)&set(r)):
    a,b=o[t],r[t]
    if [(x[0],x[1]) for x in a]!=[(x[0],x[1]) for x in b]:
        print("tick",t,"entity lists differ:",len(a),len(b)); sys.exit(1)
    bad=False
    for (gi,ci,ha),(_,_,hb) in zip(a,b):
        A=bytes.fromhex(ha); B=bytes.fromhex(hb)
        diffs=[]
        for off in range(0,0x1e4,4):
            if off in PTR or off+4>len(A): continue
            if A[off:off+4]!=B[off:off+4]:
                fa,fb=struct.unpack_from('<f',A,off)[0],struct.unpack_from('<f',B,off)[0]
                diffs.append(f"+{off:#x}: {A[off:off+4].hex()}({fa:.6g}) vs {B[off:off+4].hex()}({fb:.6g})")
        if diffs:
            unit=A[0:0].hex()
            print(f"tick {t} group {gi} child {ci}:"); print("   "+"\n   ".join(diffs[:20])); bad=True
    if bad: sys.exit(1)
print("all equal")
