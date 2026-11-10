"""plates.py OUTDIR [MAX]: from filmstart.ddck, dump every 8-bit plate the
original hands U_SpritePlate_GetBitmapRectListFromPlate (0x40e300) as
OUTDIR/plate_NNN_<w>x<h>_<stride>.bin (rows of `stride` bytes; the call's
arguments are data, stride, width, height)."""
import dd, struct, sys, os
dd.CFG["restore"]=dd.S+"/dd/filmstart.ddck"
d=dd.DD("plates")
out=sys.argv[1]; os.makedirs(out,exist_ok=True)
mx=int(sys.argv[2]) if len(sys.argv)>2 else 400
d.call("breakpoint.add",{"addr":0x40e300})
n=0
while n<mx:
    r=d.run(seconds=60)
    if 'Breakpoint' not in str(r['stop']): print("STOP",r,flush=True); break
    esp=d.call("regs.get")['regs']['esp']
    data,stride,w,h=struct.unpack('<Iiii',d.read(esp+4,16))
    buf=d.read(data,stride*h)
    p=f"{out}/plate_{n:03d}_{w}x{h}_{stride}.bin"; open(p,'wb').write(buf); print(p,flush=True)
    n+=1
d.close()
