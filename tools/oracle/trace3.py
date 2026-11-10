# trace3.py MAXGT OUT [checkpoint]: rand draws, state changes, spawns, destroys, player ticks
import dd, struct, sys
dd.CFG["restore"]=dd.S+"/dd/"+(sys.argv[3] if len(sys.argv)>3 else "filmstart.ddck")
d=dd.DD("trace3")
RS=0x110130
BP={0x4615e0:'R',0x432820:'T',0x4135a0:'C',0x417a90:'S',0x415200:'D'}
for a in BP: d.call("breakpoint.add",{"addr":a})
MAXGT=int(sys.argv[1]); out=open(dd.S+"/dd/"+sys.argv[2],"w",buffering=1)
def rid(v): return struct.pack('<I',v).decode('latin1')
def ent_unit(e):
    dp=struct.unpack('<I',d.read(e+0x8a,4))[0]
    return rid(struct.unpack('<I',d.read(dp+4,4))[0]) if dp else '????'
while True:
    r=d.run(seconds=100)
    if 'Breakpoint' not in str(r['stop']): out.write(f"STOP {r}\n"); break
    g=d.call("regs.get")['regs']; k=BP[g['eip']]; esp=g['esp']
    if k=='T':
        p=d.read(g['ecx'],0xc6); gt=d.u32(0x4e4836)
        if struct.unpack_from('<i',p,0xc2)[0]!=0: continue
        x,y=struct.unpack_from('<ff',p,0)
        score=(struct.unpack_from('<i',p,0xa6)[0]-0x5532a3e)&0xffffffff
        out.write(f"T {gt} ({x:.4f},{y:.4f}) score={score} st={struct.unpack_from('<i',p,0xba)[0]}\n")
        if gt>=MAXGT: break
    elif k=='R':
        ret=struct.unpack('<I',d.read(esp,4))[0]; ret2=struct.unpack('<I',d.read(g['ebp']+4,4))[0]
        extra=''
        if ret2==0x44f61d:
            s=d.read(g['ebx'],0x18); extra=' snd='+rid(struct.unpack_from('<I',s,0)[0])+' v=%d,%d'%struct.unpack_from('<ii',s,0x10)
        out.write(f"R {ret:08x} {ret2:08x} {d.u32(RS):08x}{extra}\n")
    elif k=='C':
        e=g['ecx']; a=d.read(esp+4,16); f1,nm,idx=a[0],struct.unpack_from('<I',a,4)[0],struct.unpack_from('<i',a,8)[0]
        name=d.read(nm,24).split(b'\0')[0].decode('latin1') if nm else ''
        x,y=struct.unpack('<ff',d.read(e,8))
        out.write(f"C {ent_unit(e)} e={e:08x} force={f1} name={name!r} idx={idx} at=({x:.2f},{y:.2f})\n")
    elif k=='S':
        rq=struct.unpack('<I',d.read(esp+4,4))[0]; q=d.read(rq,0x28)
        out.write(f"S {rid(struct.unpack_from('<I',q,0)[0])} at=({struct.unpack_from('<f',q,4)[0]:.2f},{struct.unpack_from('<f',q,8)[0]:.2f}) raw={q.hex()}\n")
    elif k=='D':
        e=g['ecx']; x,y=struct.unpack('<ff',d.read(e,8))
        out.write(f"D {ent_unit(e)} e={e:08x} at=({x:.2f},{y:.2f})\n")
d.close()
