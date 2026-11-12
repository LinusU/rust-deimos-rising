"""Action statistics of a snapshot, both ships flown by it (player 1 as
lead, player 2 as support), from level `--level` (list position, default 1):
uv run python behaviour.py runs/<run>/ckpt_XXXXX.pt [--temperature T] [--level N]"""
import argparse

import numpy as np
import torch

from envclient import I_PSTATS, PSTATS, P_CHARGES, P_HITS, P_MISSES, P_MONEY, P_VOLLEYS, P_WCHANGES, S_LATCH, S_ROLE, EnvServer
from model import Policy, sample
from train import to_dev

p = argparse.ArgumentParser()
p.add_argument("ckpt")
p.add_argument("--temperature", type=float, default=1.0)
p.add_argument("--level", type=int, default=1)
p.add_argument("--decisions", type=int, default=3000)
a = p.parse_args()
ck = torch.load(a.ckpt, map_location="cpu", weights_only=False)
n = 16
env = EnvServer(n, threads=4, seed=9, levels=[a.level])
m = Policy(env.F, env.S, env.SELF_IDS, **{k: v for k, v in ck["cfg"].items() if k not in ("F", "S", "self_ids")})
m.load_state_dict(ck["model"])
m.eval()
obs = env.read()
roles = np.tile(np.array([1, 0]), (n, 1))
acts_log, alive, latch = [], [], []
pst = np.zeros((n, 2, PSTATS))
for t in range(a.decisions):
    obs["self"] = obs["self"].copy()
    obs["self"][:, :, S_ROLE] = roles
    with torch.no_grad():
        lg, _ = m(*to_dev(obs, "cpu"))
        act, _ = sample(lg, a.temperature, heads=m.acts)
    acts_log.append(act.numpy().reshape(n, 2, 4))
    sv = obs["self"]
    alive.append(sv[:, :, 6] > 0.5)  # state 4 (flying)
    latch.append(sv[:, :, S_LATCH] > 0.5)
    from model import pack_actions
    obs = env.step(pack_actions(act).numpy().reshape(n, 2), np.ones((n, 2), bool))
    for q in range(2):
        pst[:, q] += obs["info"][:, I_PSTATS + PSTATS * q:I_PSTATS + PSTATS * (q + 1)] * alive[-1][:, q:q + 1]
env.close()
A = np.stack(acts_log)
L = np.stack(alive)
H = np.stack(latch)
for q, name in ((1, "support"), (0, "lead")):
    Lq = L[:, :, q]
    d = A[:, :, q, 0]
    both = Lq[1:] & Lq[:-1]
    secs = Lq.sum() / 10
    mins = secs / 60
    print(f"== {name} ({secs / 60:.1f} ship-minutes flying)")
    print(f"  direction changes per second: {((d[1:] != d[:-1]) & both).sum() / both.sum() * 10:.2f}")
    print("  direction histogram (none, up, ur, r, dr, d, dl, l, ul):", np.round(np.bincount(d[Lq], minlength=9) / Lq.sum(), 3))
    air = A[:, :, q, 1]
    free = Lq & ~H[:, :, q]
    print(f"  air choices when free (off, fire, charge): {np.round(np.bincount(air[free], minlength=3) / free.sum(), 3)}, "
          f"charging {H[:, :, q][Lq].mean():.2f} of the time")
    s = pst[:, q].sum(0)
    print(f"  per minute: charges {s[P_CHARGES] / mins:.2f}, weapon changes {s[P_WCHANGES] / mins:.2f}, "
          f"bomb volleys {s[P_VOLLEYS] / mins:.1f} (hits {s[P_HITS] / mins:.1f}, misses {s[P_MISSES] / mins:.1f}), money {s[P_MONEY] / mins:.1f}")
