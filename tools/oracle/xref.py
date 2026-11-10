# xref.py exe target_hex : list call sites to target in .text
import sys, struct, capstone
data=open(sys.argv[1],'rb').read()
pe=struct.unpack_from('<I',data,0x3c)[0]; nsec=struct.unpack_from('<H',data,pe+6)[0]; opt=struct.unpack_from('<H',data,pe+20)[0]
base=struct.unpack_from('<I',data,pe+24+28)[0]
o=pe+24+opt; va,rs,rp=struct.unpack_from('<III',data,o+12); vs=struct.unpack_from('<I',data,o+8)[0]
code=data[rp:rp+rs]; tgt=int(sys.argv[2],16)
for i in range(len(code)-5):
    if code[i]==0xe8:
        d=struct.unpack_from('<i',code,i+1)[0]
        if base+va+i+5+d==tgt: print(hex(base+va+i))
