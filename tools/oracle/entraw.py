"""entraw.py FROM TO OUT: at each tick in [FROM, TO] (on player 1's
G_Player::Process), walk g_entityGroupList (0x4e0d70) and each group's
child list (group+0xa6) and write every child's raw 0x1e4-byte G_Entity,
in processing order: `E tick group_idx child_idx unit hex`."""
import dd, struct, sys
dd.CFG["restore"]=dd.S+"/dd/filmstart.ddck"
d=dd.DD("entraw")
lo,hi,outp=int(sys.argv[1]),int(sys.argv[2]),sys.argv[3]
out=open(outp,"w")
d.call("breakpoint.add",{"addr":0x432820})
def objs(lst):
    r=[]; num,head,tail=struct.unpack('<iII',d.read(lst,12)); link=head
    while link and len(r)<num:
        prev,nxt,obj=struct.unpack('<III',d.read(link,12)); r.append(obj); link=nxt
    return r
while True:
    d.run(seconds=100)
    g=d.call("regs.get")['regs']
    if struct.unpack('<i',d.read(g['ecx']+0xc2,4))[0]!=0: continue
    gt=d.u32(0x4e4836)
    if gt<lo: continue
    if gt>hi: break
    for gi,grp in enumerate(objs(d.u32(0x4e0d70))):
        for ci,e in enumerate(objs(d.u32(grp+0xa6))):
            b=d.read(e,0x1e4); dp=struct.unpack_from('<I',b,0x8a)[0]
            u=d.read(dp+4,4).decode('latin1') if dp else '????'
            out.write(f"E {gt} {gi} {ci} {u} {b.hex()}\n")
    out.flush()
d.close()
