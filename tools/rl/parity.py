"""Checks the Rust forward pass (src/rl/policy.rs) against the PyTorch model:
uv run python parity.py runs/<run>/ckpt_XXXXX.pt"""

import os
import struct
import subprocess
import sys
import tempfile

import numpy as np
import torch

from envclient import BIN, ROOT, EnvServer
from model import Policy, export


def main():
    ck = torch.load(sys.argv[1], map_location="cpu", weights_only=False)
    env = EnvServer(8, threads=4, seed=5)
    m = Policy(env.F, env.S, env.SELF_IDS, **{k: v for k, v in ck["cfg"].items() if k not in ("F", "S", "self_ids")})
    m.load_state_dict(ck["model"])
    m.eval()
    obs = env.read()
    rng = np.random.default_rng(0)
    rows = []
    for t in range(60):
        obs = env.step(rng.integers(0, 128, size=(8, 2), dtype=np.uint8) | 128)
        if t % 10 == 9:
            rows.append({k: obs[k].copy() for k in ("ent", "ent_id", "self", "self_id")})
    env.close()
    with tempfile.TemporaryDirectory() as d:
        pol = os.path.join(d, "p.bin")
        dump = os.path.join(d, "dump.bin")
        export(m, pol)
        with open(dump, "wb") as f:
            n = sum(r["ent"].shape[0] * 2 for r in rows)
            f.write(struct.pack("<ii", n, 15))
            for r in rows:
                A = r["ent"].shape[0] * 2
                t = [torch.from_numpy(r[k].reshape(A, *r[k].shape[2:]).copy()) for k in ("ent", "ent_id", "self", "self_id")]
                t[2] = t[2].view(A, -1)
                t[3] = t[3].view(A, -1)
                with torch.no_grad():
                    lg, v = m(*t)
                for a in range(A):
                    f.write(t[0][a].numpy().astype("<f4").tobytes())
                    f.write(t[1][a].numpy().astype("<i4").tobytes())
                    f.write(t[2][a].numpy().astype("<f4").tobytes())
                    f.write(t[3][a].numpy().astype("<i4").tobytes())
                    f.write(np.concatenate([lg[a].numpy(), v[a:a + 1].numpy()]).astype("<f4").tobytes())
        print(subprocess.run([BIN, "--rl-parity", pol, dump], cwd=ROOT, capture_output=True, text=True).stdout)


if __name__ == "__main__":
    main()
