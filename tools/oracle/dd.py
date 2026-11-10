"""Drive the original Deimos Rising under Drydock (`drydock serve --stdio`).

Checkpoints and traces go to $DD_OUT/dd (default target/oracle/dd). Film
traces start from `filmstart.ddck`, saved right as Demo 01 starts playing:
run 18 s, click DEMOS at (324,341), stop on G_Film::StartPlayback 0x41e210.
"""
import json, subprocess, os, sys
# Output directory for checkpoints, traces and screenshots.
S=os.environ.get("DD_OUT", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "target", "oracle"))
os.makedirs(S+"/dd", exist_ok=True)
BIN=os.path.expanduser('~/coding/drydock86/target/release/drydock')
CFG={
 "exe":"DeimosRising.exe","root":os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "retail"),"audio":"discard","no_persist":True,
 "overlay":S+"/dd/ov",
 "map":["C:\\GAME\\QuickTime=/Users/linus/Library/Application Support/CrossOver/Bottles/Deimos Rising/drive_c/windows/system32/QuickTime",
  "C:\\Documents and Settings\\Player\\Application Data\\Deimos Rising License.lcs=/Users/linus/Library/Application Support/CrossOver/Bottles/Deimos Rising/drive_c/users/crossover/AppData/Roaming/Deimos Rising License.lcs"]}
class DD:
    def __init__(self, name="run"):
        self.p=subprocess.Popen([BIN,'serve','--stdio'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=open(S+'/dd/stderr.log','a'),text=True,bufsize=1)
        self.i=0
        c=dict(CFG); c["artifacts"]=S+"/dd/"+name
        self.open=self.call("open",c)
    def call(self,m,params=None):
        self.i+=1
        self.p.stdin.write(json.dumps({"id":self.i,"method":m,"params":params or {}})+"\n"); self.p.stdin.flush()
        r=json.loads(self.p.stdout.readline())
        if "error" in r: raise RuntimeError(f"{m}: {r['error']}")
        return r["result"]
    def run(self,**kw): return self.call("run",kw)
    def read(self,addr,n): return bytes.fromhex(self.call("memory.read",{"addr":addr,"len":n})["hex"])
    def u32(self,a): return int.from_bytes(self.read(a,4),'little')
    def shot(self,path): return self.call("screenshot",{"path":path})
    def step(self,st): return self.call("step",{"step":st})
    def close(self):
        try: self.call("shutdown")
        except Exception: pass
