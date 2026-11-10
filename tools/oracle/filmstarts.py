"""filmstarts.py: from filmstart.ddck (Demo 01 starting), keep playing the
demos and save filmstart2..4.ddck as each next film starts. The menu needs
a click on DEMOS (324,341) after every film."""
import dd
dd.CFG["restore"]=dd.S+"/dd/filmstart.ddck"
d=dd.DD("filmstarts")
d.call("breakpoint.add",{"addr":0x41e210})
n=2
while n<=4:
    r=d.run(seconds=20)
    if 'Breakpoint' in str(r['stop']):
        d.call("checkpoint.save",{"path":dd.S+f"/dd/filmstart{n}.ddck","label":f"film{n}"})
        print("film",n,"at",r['virtual_s'],flush=True); n+=1
        d.run(seconds=0.1)
    else:
        d.step({"click":[324,341]})
d.close()
