"""Plays saved snapshots through whole games from level 1 and records how far
they get, to pick a co-pilot that helps without carrying the game:

- camper: the bot flies player 2; player 1 flies to a bottom corner and
  stays there without firing (the "user hides in the corner" case). A good
  co-pilot should NOT finish the game like this.
- duo:    the bot flies both ships, player 1 in the lead role and player 2
          as support (a stand-in for a partner who plays).

uv run python eval.py runs/<run> [--every 2] [--games 24] [--only 00500,01000]
Appends to runs/<run>/eval.jsonl and prints a table.
"""

import argparse
import glob
import json
import os

import numpy as np
import torch

from envclient import (EnvServer, I_DSCORE, I_END, I_LEVEL, I_LEVELS_DONE, I_PROGRESS, I_PSTATS, I_TICKS, P_CHARGES,
                       P_WCHANGES, PSTATS, S_ROLE)
from model import Policy, pack_actions, sample
from train import Scripted, to_dev


def evaluate(ck_path, games, dev, temperature=1.0, seed=123):
    ck = torch.load(ck_path, map_location="cpu", weights_only=False)
    n = games * 2
    env = EnvServer(n, threads=16, seed=seed, max_ticks=10 ** 9, max_levels=99, levels=[1])
    cfg = {k: v for k, v in ck["cfg"].items() if k not in ("F", "S", "self_ids")}
    m = Policy(env.F, env.S, env.SELF_IDS, **cfg).to(dev)
    m.load_state_dict(ck["model"])
    m.eval()
    rng = np.random.default_rng(seed)
    scripted = Scripted(n, rng)
    camper = np.arange(n) < games
    for e in range(n):
        scripted.reset(e)
        scripted.fire[e] = False
        scripted.target[e] = (rng.choice([0.06, 0.94]), 0.92)
    learner = np.zeros((n, 2), bool)
    learner[:, 1] = True
    learner[~camper, 0] = True
    roles = np.zeros((n, 2), np.int64)
    roles[~camper, 0] = 1
    support_stats = np.zeros((n, 2))  # charges, weapon changes of player 2
    finished = np.zeros(n, bool)
    res = [None] * n
    score = np.zeros(n)
    max_level = np.zeros(n)
    obs = env.read()
    while not finished.all():
        obs["self"] = obs["self"].copy()
        obs["self"][:, :, S_ROLE] = roles
        o = to_dev(obs, dev)
        with torch.no_grad():
            lg, _ = m(*o)
            acts, _ = sample(lg, temperature, heads=m.acts)
            b = pack_actions(acts).cpu().numpy()
        selfv = obs["self"].reshape(2 * n, -1)
        es = np.nonzero(camper)[0]
        b[es * 2] = scripted.camper(es, selfv[es * 2])
        obs = env.step(b.reshape(n, 2), learner)
        info = obs["info"]
        live = ~finished
        o2 = I_PSTATS + PSTATS
        support_stats[live] += info[live][:, [o2 + P_CHARGES, o2 + P_WCHANGES]]
        score[live] += info[live, I_DSCORE] + info[live, 8 + I_DSCORE]
        max_level[live] = np.maximum(max_level[live], info[live, I_LEVEL])
        for e in np.nonzero(live & (info[:, I_END] > 0))[0]:
            finished[e] = True
            res[e] = dict(levels=float(info[e, I_LEVELS_DONE]), end=float(info[e, I_END]), score=float(score[e]),
                          minutes=float(info[e, I_TICKS]) / 30 / 60, progress=float(info[e, I_PROGRESS]),
                          level=float(max_level[e]), charges=float(support_stats[e, 0]), wchanges=float(support_stats[e, 1]))
    env.close()
    out = {}
    for name, sel in (("camper", camper), ("duo", ~camper)):
        rs = [r for r, s in zip(res, sel) if s]
        out[name] = {
            "levels_mean": float(np.mean([r["levels"] for r in rs])),
            "levels_max": float(np.max([r["levels"] for r in rs])),
            "reached_level_mean": float(np.mean([r["level"] for r in rs])),
            "won": float(np.mean([r["end"] == 2 for r in rs])),
            "score": float(np.mean([r["score"] for r in rs])),
            "minutes": float(np.mean([r["minutes"] for r in rs])),
            "support_charges_pm": float(np.sum([r["charges"] for r in rs]) / max(1e-6, np.sum([r["minutes"] for r in rs]))),
            "support_wchanges_pm": float(np.sum([r["wchanges"] for r in rs]) / max(1e-6, np.sum([r["minutes"] for r in rs]))),
        }
    return out


def main():
    p = argparse.ArgumentParser()
    p.add_argument("run")
    p.add_argument("--every", type=int, default=1)
    p.add_argument("--games", type=int, default=24)
    p.add_argument("--only", default=None)
    p.add_argument("--temperature", type=float, default=1.0)
    p.add_argument("--device", default="mps" if torch.backends.mps.is_available() else "cpu")
    a = p.parse_args()
    done = set()
    path = os.path.join(a.run, "eval.jsonl")
    if os.path.exists(path):
        for l in open(path):
            r = json.loads(l)
            done.add((r["ckpt"], r["temperature"], r["games"]))
    cks = sorted(glob.glob(os.path.join(a.run, "ckpt_*.pt")))[:: a.every]
    if a.only:
        want = set(a.only.split(","))
        cks = [c for c in cks if os.path.basename(c)[5:10] in want]
    for c in cks:
        key = (os.path.basename(c), a.temperature, a.games)
        if key in done:
            continue
        r = evaluate(c, a.games, torch.device(a.device), a.temperature)
        rec = {"ckpt": key[0], "temperature": a.temperature, "games": a.games, **r}
        with open(path, "a") as f:
            f.write(json.dumps(rec) + "\n")
        print(f"{key[0]} T={a.temperature}  camper: levels {r['camper']['levels_mean']:.2f} (max {r['camper']['levels_max']:.0f}) "
              f"won {r['camper']['won']:.2f} score {r['camper']['score']:.0f}  |  duo: levels {r['duo']['levels_mean']:.2f} "
              f"(max {r['duo']['levels_max']:.0f}) won {r['duo']['won']:.2f} score {r['duo']['score']:.0f}  |  support charges/min "
              f"{r['duo']['support_charges_pm']:.2f} weapon changes/min {r['duo']['support_wchanges_pm']:.2f}", flush=True)


if __name__ == "__main__":
    main()
