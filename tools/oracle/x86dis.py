import sys, struct, capstone
data=open(sys.argv[1],'rb').read()
pe=struct.unpack_from('<I',data,0x3c)[0]
nsec=struct.unpack_from('<H',data,pe+6)[0]; opt=struct.unpack_from('<H',data,pe+20)[0]
base=struct.unpack_from('<I',data,pe+24+28)[0]
secs=[]
for i in range(nsec):
    o=pe+24+opt+40*i
    va,rs,rp=struct.unpack_from('<III',data,o+12); vs=struct.unpack_from('<I',data,o+8)[0]
    secs.append((base+va,vs,rp,rs))
def off(a):
    for va,vs,rp,rs in secs:
        if va<=a<va+max(vs,rs): return rp+a-va
addr=int(sys.argv[2],16); n=int(sys.argv[3]) if len(sys.argv)>3 else 60
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
o=off(addr)
for i,ins in enumerate(md.disasm(data[o:o+n*16],addr)):
    if i>=n: break
    print(f"{ins.address:08x}  {ins.mnemonic} {ins.op_str}")
