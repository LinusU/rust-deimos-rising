"""framerecs.py TICK: at that tick, print each entity's sprite id, frame and
the frame record it points to (+0x4e): the original's own frame sizes."""
import dd, struct, sys
dd.CFG["restore"]=dd.S+"/dd/filmstart.ddck"
d=dd.DD("framerecs")
want=int(sys.argv[1])
d.call("breakpoint.add",{"addr":0x432820})
def objs(lst):
    r=[]; num,head,tail=struct.unpack('<iII',d.read(lst,12)); link=head
    while link and len(r)<num:
        prev,nxt,obj=struct.unpack('<III',d.read(link,12)); r.append(obj); link=nxt
    return r
while True:
    d.run(seconds=100)
    if d.u32(0x4e4836)>=want: break
seen=set()
for grp in objs(d.u32(0x4e0d70)):
    for e in objs(d.u32(grp+0xa6)):
        b=d.read(e,0x60); sid=b[0x1c:0x20].decode('latin1'); fr=struct.unpack_from('<i',b,0x20)[0]
        p=struct.unpack_from('<I',b,0x4e)[0]
        if (sid,fr) in seen or not p: continue
        seen.add((sid,fr)); rec=d.read(p,0x20)
        print(sid, fr, struct.unpack_from('<iii',rec,0), rec.hex())
d.close()
