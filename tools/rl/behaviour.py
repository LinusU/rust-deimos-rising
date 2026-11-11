"""Action statistics of a snapshot (both ships flown by it, level 1):
uv run python behaviour.py runs/<run>/ckpt_XXXXX.pt [temperature]"""
import sys

import numpy as np
import torch

from envclient import EnvServer
from model import Policy, sample
from train import to_dev

ck = torch.load(sys.argv[1], map_location="cpu", weights_only=False)
env = EnvServer(16, threads=4, seed=9, levels=[1])
m = Policy(env.F, env.S, env.SELF_IDS, **{k: v for k, v in ck["cfg"].items() if k not in ("F", "S", "self_ids")})
m.load_state_dict(ck["model"])
m.eval()
obs = env.read()
acts_log = []
alive = []
for t in range(1500):
    with torch.no_grad():
        lg, _ = m(*to_dev(obs, "cpu"))
        a, _ = sample(lg, float(sys.argv[2]) if len(sys.argv) > 2 else 1.0)
    acts_log.append(a.numpy())
    alive.append(obs["self"].reshape(32, -1)[:, 6] > 0.5)  # state 4 (active)
    b = (a[:, 0] | a[:, 1] << 4 | a[:, 2] << 5 | a[:, 3] << 6).numpy().astype(np.uint8) | 128
    obs = env.step(b.reshape(16, 2))
env.close()
A = np.stack(acts_log)
L = np.stack(alive)
d = A[..., 0]
ch = (d[1:] != d[:-1]) & L[1:] & L[:-1]
print(f"direction changes per second: {ch.sum() / (L[1:] & L[:-1]).sum() * 10:.2f}")
print("direction histogram (none, up, ur, r, dr, d, dl, l, ul):", np.round(np.bincount(d[L], minlength=9) / L.sum(), 3))
print(f"air button held: {A[..., 1][L].mean():.2f}, ground held: {A[..., 2][L].mean():.2f}, cycle: {A[..., 3][L].mean():.3f}")
air = A[..., 1]
presses = ((air[1:] == 1) & (air[:-1] == 0) & L[1:]).sum() / L.sum() * 10
print(f"air presses per second: {presses:.2f}")
