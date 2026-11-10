"""unitdefs.py OUTDIR ID...: dump the original's G_UnitDef blocks (0x79e0
bytes, found by their 0x499602d2 signature plus id) at film start."""
import dd, sys, os
dd.CFG["restore"]=dd.S+"/dd/filmstart.ddck"
d=dd.DD("unitdefs")
out=sys.argv[1]; os.makedirs(out, exist_ok=True)
for u in sys.argv[2:]:
    hexs="d2029649"+u.encode('latin1').hex()
    hits=d.call("memory.search",{"hex":hexs,"limit":4})["hits"]
    if not hits: print(u,"not loaded"); continue
    a=int(hits[0],0) if isinstance(hits[0],str) else hits[0]
    open(os.path.join(out,u.replace(' ','_')+".orig"),"wb").write(d.read(a,0x79e0))
    print(u, hex(a), len(hits))
d.close()
