# ents.py TICK [TICK2...]: dump entities processed in G_EG_Process at those ticks
import dd, struct, sys
dd.CFG["restore"]=dd.S+"/dd/filmstart.ddck"
d=dd.DD("ents")
ticks=sorted(int(x) for x in sys.argv[1:])
def rid(v): return struct.pack('<I',v).decode('latin1')
d.call("breakpoint.add",{"addr":0x432820})
gt=-1
while ticks:
    r=d.run(seconds=100)
    g=d.call("regs.get")['regs']
    if g['eip']==0x432820:
        gt=d.u32(0x4e4836)
        if gt==ticks[0]:
            d.call("breakpoint.add",{"addr":0x418663}); armed=True; print(f"== tick {gt}")
            p=d.read(g['ecx'],8); print("player", struct.unpack('<ff',p))
        elif gt>ticks[0]:
            d.call("breakpoint.remove",{"addr":0x418663}); ticks.pop(0)
            if ticks and gt==ticks[0]:
                d.call("breakpoint.add",{"addr":0x418663}); print(f"== tick {gt}")
    else:
        e=g['esi']; b=d.read(e,0x100)
        dp=struct.unpack_from('<I',b,0x8a)[0]; u=rid(struct.unpack('<I',d.read(dp+4,4))[0])
        x,y=struct.unpack_from('<ff',b,0); vx,vy=struct.unpack_from('<ff',b,0x10)
        print(f"  {u} e={e:08x} pos=({x:.3f},{y:.3f}) f10=({vx:.3f},{vy:.3f}) st={struct.unpack_from('<i',b,0x9e)[0]} raw={b[0x9e:0x100].hex()}")
d.close()
