"""spritedims.py TICK: at that Demo 01 tick, print every loaded sprite's
frame sizes from the original's memory (g_spriteGroupList at 0x4dce06):
`<id> w1xh1 w2xh2 ...`. Compare with `deimos --sprite-dims <id>`."""
import dd, struct, sys
dd.CFG["restore"]=dd.S+"/dd/filmstart.ddck"
d=dd.DD("spritedims")
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
for g in objs(d.u32(0x4dce06)):
    magic,sid,n,fd=struct.unpack('<IIiI',d.read(g,16))
    if magic!=0x499602d2 or n<=0 or n>4096: continue
    ptrs=struct.unpack(f'<{n}I',d.read(fd,4*n))
    dims=[]
    for p in ptrs:
        m,w,h=struct.unpack('<Iii',d.read(p,12)); dims.append(f"{w}x{h}")
    print(struct.pack('<I',sid).decode('latin1'), ' '.join(dims), flush=True)
d.close()
