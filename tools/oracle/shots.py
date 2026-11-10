"""shots.py OUTDIR TICK...: screenshots of the original at those Demo 01
game ticks (taken when player 1's G_Player::Process starts), cropped to
the 640x480 client area."""
import dd, sys, struct, os, subprocess
dd.CFG["restore"]=dd.S+"/dd/filmstart.ddck"
d=dd.DD("shots")
out=sys.argv[1]; os.makedirs(out,exist_ok=True)
ticks=sorted(int(t) for t in sys.argv[2:])
d.call("breakpoint.add",{"addr":0x432820})
while ticks:
    d.run(seconds=100)
    gt=d.u32(0x4e4836)
    if gt>=ticks[0]:
        # Let the frame for this tick finish drawing and present.
        d.call("breakpoint.remove",{"addr":0x432820})
        d.run(until_present=True, seconds=1)
        p=f"{out}/orig_{ticks[0]:05d}.png"; d.shot(p)
        subprocess.run(["sips","-c","480","640","--cropOffset","23","4",p,"--out",p],capture_output=True)
        print(p,flush=True); ticks.pop(0)
        d.call("breakpoint.add",{"addr":0x432820})
d.close()
