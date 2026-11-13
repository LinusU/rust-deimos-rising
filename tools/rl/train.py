"""PPO for the co-pilot.

Each game has two ships. One network plays two roles, told apart by an
input: the support (the co-pilot) and the lead (a stand-in for a person
who goes for ground targets and coins and takes more risks). Per episode
a game draws a partner setup:
- duo:    one ship plays lead, the other support (both learn)
- past:   the support learns next to a frozen earlier snapshot as lead
- camper: the partner flies to a spot near the bottom and stays there
          (sometimes firing): the "user hides in the corner" case
- idle:   the partner never touches the controls
- noisy:  the partner wanders like a distracted beginner
Training against a population of partners of mixed skill rather than only
itself keeps the bot from depending on conventions only it would follow
(Strouse et al. 2021, "Collaborating with Humans without Human Data").

The reward is cooperative: a ship's own terms plus half of its partner's
(score, lives lost, shield damage). The support also pays for its
partner's damage and deaths (--protect) and for bomb volleys that score
nothing; the lead gets a bonus for volleys that score and for money, pays
a little for volleys that miss, and has a smaller death penalty. These
bombing and money terms are not shared with the partner. Full charges
get a bonus that fades out
(--charge-bonus over --charge-anneal updates): a charge costs ~2 s of
fire before it pays, so the bot would hardly try one otherwise. A charge
let go in a lull (no points by either ship for --quiet-secs) keeps a
bonus (--quiet-charge): laying the Rear Gun's bubbles before a wave is
what good players do with downtime. Every air weapon change costs a
little (--change-cost), against switching faster than a person would.

Usage: uv run python train.py --run NAME [--envs 256 ...]
"""

import argparse
import collections
import json
import os
import random
import time

import numpy as np
import torch

from envclient import (EnvServer, I_DAMAGE, I_DEATHS, I_DSCORE, I_END, I_LEVEL_DONE, I_LEVELS_DONE, I_PROGRESS,
                       I_PSTATS, I_START_LEVEL, I_TICKS, P_CHARGES, P_HITS, P_MISSES, P_MONEY, P_VOLLEYS, P_WCHANGES,
                       PSTATS, S_INGAME, S_LATCH, S_ROLE)
from model import Policy, evaluate, export, masked_logp, pack_actions, sample

MODES = ["duo", "past", "camper", "idle", "noisy"]
ROLES = ["support", "lead"]


def parse():
    p = argparse.ArgumentParser()
    p.add_argument("--run", default="run1")
    p.add_argument("--envs", type=int, default=256)
    p.add_argument("--threads", type=int, default=16)
    p.add_argument("--steps", type=int, default=128, help="decisions per rollout")
    p.add_argument("--updates", type=int, default=100000)
    p.add_argument("--lr", type=float, default=3e-4)
    p.add_argument("--gamma", type=float, default=0.995)
    p.add_argument("--lam", type=float, default=0.95)
    p.add_argument("--clip", type=float, default=0.2)
    p.add_argument("--epochs", type=int, default=3)
    p.add_argument("--minibatches", type=int, default=4)
    p.add_argument("--ent", type=float, default=0.01)
    p.add_argument("--vf", type=float, default=0.5)
    p.add_argument("--team", type=float, default=0.5, help="weight of the partner's reward terms")
    p.add_argument("--death", type=float, default=3.0)
    p.add_argument("--damage", type=float, default=1.0, help="per 100%% shields lost")
    p.add_argument("--score", type=float, default=1e-3, help="per point")
    p.add_argument("--level", type=float, default=3.0)
    p.add_argument("--jitter", type=float, default=0.01, help="cost of changing direction (or letting go)")
    p.add_argument("--cycle-cost", type=float, default=0.0, help="per decision holding weapon select")
    p.add_argument("--change-cost", type=float, default=0.05, help="per air weapon change")
    p.add_argument("--quiet-charge", type=float, default=0.15, help="per full charge let go in a lull")
    p.add_argument("--quiet-secs", type=float, default=1.5, help="seconds without points that make a lull")
    p.add_argument("--protect", type=float, default=0.5, help="support: extra weight of the partner's damage and deaths")
    p.add_argument("--bomb-miss", type=float, default=0.05, help="support: cost of a bomb volley that scores nothing")
    p.add_argument("--bomb-hit", type=float, default=0.15, help="lead: bonus for a bomb volley that scores")
    p.add_argument("--lead-bomb-miss", type=float, default=0.05, help="lead: cost of a bomb volley that scores nothing")
    p.add_argument("--coin", type=float, default=0.02, help="lead: bonus per unit of money")
    p.add_argument("--lead-death", type=float, default=2.0)
    p.add_argument("--charge-bonus", type=float, default=0.0, help="per full charge let go, fading out")
    p.add_argument("--charge-anneal", type=int, default=1500, help="updates over which the charge bonus fades")
    p.add_argument("--ent-heads", default="1,1,1,1", help="entropy weight per head (direction, air, ground, cycle)")
    p.add_argument("--init", default=None, help="warm start from an older run's checkpoint")
    p.add_argument("--mode-probs", default="0.45,0.2,0.15,0.05,0.15")
    p.add_argument("--snapshot-every", type=int, default=25, help="updates between partner-pool snapshots")
    p.add_argument("--save-every", type=int, default=50, help="updates between kept checkpoints")
    p.add_argument("--pool", type=int, default=24)
    p.add_argument("--resume", default=None)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--device", default="mps" if torch.backends.mps.is_available() else "cpu")
    return p.parse_args()


class RunningStd:
    def __init__(self):
        self.n, self.mean, self.m2 = 1e-4, 0.0, 1.0

    def update(self, x):
        x = np.asarray(x, dtype=np.float64).ravel()
        if x.size == 0:
            return
        bn, bm, bv = x.size, x.mean(), x.var()
        d = bm - self.mean
        tot = self.n + bn
        self.mean += d * bn / tot
        self.m2 = (self.m2 * self.n + bv * bn + d * d * self.n * bn / tot) / tot
        self.n = tot

    @property
    def std(self):
        return float(np.sqrt(self.m2) + 1e-8)


class Scripted:
    """Scripted partners, per env."""

    def __init__(self, n, rng):
        self.rng = rng
        self.target = np.zeros((n, 2), np.float32)
        self.fire = np.zeros(n, bool)
        self.dir = np.zeros(n, np.int64)
        self.air = np.zeros(n, bool)

    def reset(self, e):
        r = self.rng
        self.target[e] = (r.choice([0.06, 0.94, r.uniform(0.1, 0.9)]), r.uniform(0.75, 0.95))
        self.fire[e] = r.random() < 0.5
        self.dir[e] = r.integers(0, 9)
        self.air[e] = r.random() < 0.7

    # Direction index for (dx + 1, dy + 1).
    DIRS = np.array([[8, 7, 6], [1, 0, 5], [2, 3, 4]])

    def camper(self, es, selfv):
        """Action bytes for envs `es` whose partner ships have `selfv` [len(es), S]."""
        x, y = selfv[:, 0], selfv[:, 1]
        tx, ty = self.target[es, 0], self.target[es, 1]
        dx = np.where(np.abs(tx - x) < 0.02, 0, np.where(tx > x, 1, -1))
        dy = np.where(np.abs(ty - y) < 0.02, 0, np.where(ty > y, 1, -1))
        return (self.DIRS[dx + 1, dy + 1] | np.where(self.fire[es], 16, 0)).astype(np.uint8)

    def noisy(self, es, selfv):
        r = self.rng
        n = len(es)
        ch = r.random(n) < 0.08
        self.dir[es[ch]] = r.integers(0, 9, ch.sum())
        fl = r.random(n) < 0.05
        self.air[es[fl]] = ~self.air[es[fl]]
        d = self.dir[es].copy()
        # Stay off the top edge like a person would.
        d[(selfv[:, 1] < 0.2) & np.isin(d, (1, 2, 8))] = 5
        return (d | np.where(self.air[es], 16, 0) | np.where(r.random(n) < 0.05, 32, 0)).astype(np.uint8)


def warm_start(model, path):
    """Loads an older run's weights into `model`: new self inputs (charge
    running, role) start at zero weight, the new air option (charge) and a
    reset weapon-select head start rarely chosen."""
    ck = torch.load(path, map_location="cpu", weights_only=False)
    old, new = ck["model"], model.state_dict()
    S0, S1 = ck["cfg"]["S"], model.cfg["S"]
    out = {}
    for k, v in old.items():
        if k == "self_in.weight":
            w = torch.zeros_like(new[k])
            w[:, :S0] = v[:, :S0]
            w[:, S1:] = v[:, S0:]
            out[k] = w
        elif k in ("pi.weight", "pi.bias"):
            # old rows: dir 0-8, air 9-10, ground 11-12, cycle 13-14
            off, fire, g0, g1, c0, c1 = v[9], v[10], v[11], v[12], v[13], v[14]
            charge = off.clone()
            if k == "pi.bias":
                charge = charge - 3.5
                cm = (c0 + c1) / 2
                c0, c1 = cm, cm - 4.5
            else:
                cm = (c0 + c1) / 2
                c0, c1 = cm, cm
            out[k] = torch.cat([v[:9], torch.stack([off, fire, charge, g0, g1, c0, c1])])
        else:
            out[k] = v
    model.load_state_dict(out)
    return ck.get("update", 0)


def to_dev(obs, dev):
    n = obs["ent"].shape[0]
    A = n * 2
    return (
        torch.from_numpy(np.ascontiguousarray(obs["ent"])).to(dev).view(A, *obs["ent"].shape[2:]),
        torch.from_numpy(np.ascontiguousarray(obs["ent_id"])).to(dev).view(A, *obs["ent_id"].shape[2:]),
        torch.from_numpy(np.ascontiguousarray(obs["self"])).to(dev).view(A, -1),
        torch.from_numpy(np.ascontiguousarray(obs["self_id"])).to(dev).view(A, -1),
    )


def main():
    a = parse()
    torch.manual_seed(a.seed)
    rng = np.random.default_rng(a.seed)
    run_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "runs", a.run)
    os.makedirs(run_dir, exist_ok=True)
    with open(os.path.join(run_dir, "args.json"), "w") as f:
        json.dump(vars(a), f, indent=1)
    dev = torch.device(a.device)

    env = EnvServer(a.envs, threads=a.threads, seed=a.seed)
    n, A = env.n, env.n * 2
    model = Policy(env.F, env.S, env.SELF_IDS, k=env.K).to(dev)
    opt = torch.optim.Adam(model.parameters(), lr=a.lr, eps=1e-5)
    start_update = 0
    pool = []  # (update, state_dict on cpu)
    total_steps = 0
    if a.init and not a.resume:
        print(f"warm start from {a.init} (update {warm_start(model, a.init)})")
    if a.resume:
        ck = torch.load(a.resume, map_location="cpu", weights_only=False)
        model.load_state_dict(ck["model"])
        if "opt" in ck:
            opt.load_state_dict(ck["opt"])
        start_update = ck.get("update", 0)
        total_steps = ck.get("total_steps", 0)
        pool = ck.get("pool", [])
    print(f"params: {sum(p.numel() for p in model.parameters())}")

    probs = np.array([float(x) for x in a.mode_probs.split(",")])
    probs /= probs.sum()
    mode = np.zeros(n, np.int64)
    learner = np.zeros((n, 2), bool)
    role = np.zeros((n, 2), np.int64)
    ent_heads = torch.tensor([float(x) for x in a.ent_heads.split(",")], device=dev)
    snap_of = np.zeros(n, np.int64)
    scripted = Scripted(n, rng)
    partner_models = []

    def new_episode(e):
        m = rng.choice(len(MODES), p=probs)
        if MODES[m] == "past" and not partner_models:
            m = 0
        mode[e] = m
        s = rng.integers(0, 2)
        if MODES[m] == "duo":
            learner[e] = (True, True)
            role[e] = (1, 0) if s == 0 else (0, 1)
        else:
            # The support learns; a past snapshot partner plays lead.
            learner[e] = (s == 0, s == 1)
            role[e] = (0, 1) if s == 0 else (1, 0)
            if MODES[m] != "past":
                role[e] = 0
        snap_of[e] = rng.integers(0, max(1, len(partner_models)))
        scripted.reset(e)

    for e in range(n):
        new_episode(e)

    rew_std = RunningStd()
    ret_acc = np.zeros(A)
    obs = env.read()
    ep_stats = collections.defaultdict(list)
    ep_acc = {"score": np.zeros(n), "deaths": np.zeros((n, 2)), "rew": np.zeros(A), "pst": np.zeros((n, 2, PSTATS))}
    prev_dir = np.zeros(A, np.int64)
    quiet = np.zeros(n, np.int64)  # decisions since either ship scored
    quiet_steps = int(round(a.quiet_secs * 10))
    log = open(os.path.join(run_dir, "log.jsonl"), "a")

    T = a.steps
    K, F, S, SI = env.K, env.F, env.S, env.SELF_IDS * 2
    buf_ent = torch.zeros(T, A, K, F, device=dev)
    buf_ent_id = torch.zeros(T, A, K, 2, dtype=torch.int32, device=dev)
    buf_self = torch.zeros(T, A, S, device=dev)
    buf_self_id = torch.zeros(T, A, SI, dtype=torch.int32, device=dev)
    buf_act = torch.zeros(T, A, 4, dtype=torch.long, device=dev)
    buf_logp = torch.zeros(T, A, device=dev)
    buf_val = torch.zeros(T, A, device=dev)
    buf_rew = np.zeros((T, A), np.float32)
    buf_done = np.zeros((T, A), np.float32)
    buf_valid = np.zeros((T, A), bool)
    buf_mask = torch.zeros(T, A, dtype=torch.bool, device=dev)

    for update in range(start_update, a.updates):
        t0 = time.time()
        # Partners from the pool for this rollout.
        if pool:
            picks = [pool[i] for i in rng.choice(len(pool), size=min(2, len(pool)), replace=False)]
            partner_models = []
            for _, sd in picks:
                pm = Policy(env.F, env.S, env.SELF_IDS, k=env.K).to(dev)
                pm.load_state_dict(sd)
                pm.eval()
                partner_models.append(pm)
            snap_of[:] = rng.integers(0, len(partner_models), size=n)
        model.eval()
        t_env = 0.0
        cbonus = a.charge_bonus * max(0.0, 1.0 - (update - start_update) / max(1, a.charge_anneal))
        for t in range(T):
            obs["self"] = obs["self"].copy()
            obs["self"][:, :, S_ROLE] = role
            o = to_dev(obs, dev)
            air_mask = o[2][:, S_LATCH] > 0.5
            with torch.no_grad():
                logits, value = model(*o)
                acts, _ = sample(logits, heads=model.acts)
                logp = masked_logp(logits, acts, air_mask, model.acts)
                bytes_ = pack_actions(acts).cpu().numpy()
            learn_flat = learner.reshape(-1)
            selfv = obs["self"].reshape(A, -1)
            pslot_ai = np.arange(n) * 2 + np.where(learner[:, 1], 0, 1)
            for md, fn in (("camper", scripted.camper), ("noisy", scripted.noisy), ("idle", None)):
                es = np.nonzero(mode == MODES.index(md))[0]
                if len(es):
                    ai = pslot_ai[es]
                    bytes_[ai] = 0 if fn is None else fn(es, selfv[ai])
            past_envs = np.nonzero(mode == MODES.index("past"))[0]
            if len(past_envs) and partner_models:
                with torch.no_grad():
                    for si, pm in enumerate(partner_models):
                        es = past_envs[snap_of[past_envs] == si]
                        if len(es) == 0:
                            continue
                        slots = np.where(learner[es, 1], 0, 1)
                        idx = torch.from_numpy(es * 2 + slots).to(dev)
                        lg, _ = pm(o[0][idx], o[1][idx], o[2][idx], o[3][idx])
                        pa, _ = sample(lg, heads=pm.acts)
                        bytes_[(es * 2 + slots)] = pack_actions(pa).cpu().numpy()
            buf_ent[t] = o[0]
            buf_ent_id[t] = o[1]
            buf_self[t] = o[2]
            buf_self_id[t] = o[3]
            buf_act[t] = acts
            buf_logp[t] = logp
            buf_val[t] = value
            buf_mask[t] = air_mask
            in_game = selfv[:, S_INGAME] > 0.5
            buf_valid[t] = learn_flat & in_game
            te = time.time()
            obs = env.step(bytes_.reshape(n, 2), learner)
            t_env += time.time() - te
            info = obs["info"]

            # Rewards.
            own = np.zeros((n, 2), np.float32)
            harm = np.zeros((n, 2), np.float32)
            pst = np.zeros((n, 2, PSTATS), np.float32)
            shape = np.zeros((n, 2), np.float32)  # bombing and coin terms, not shared with the partner
            for p in range(2):
                b = p * 8
                pst[:, p] = info[:, I_PSTATS + PSTATS * p:I_PSTATS + PSTATS * (p + 1)]
                lead = role[:, p] == 1
                harm[:, p] = a.death * info[:, b + I_DEATHS] + a.damage * info[:, b + I_DAMAGE] / 100.0
                own[:, p] = (a.score * info[:, b + I_DSCORE]
                             - np.where(lead, a.lead_death, a.death) * info[:, b + I_DEATHS]
                             - a.damage * info[:, b + I_DAMAGE] / 100.0
                             + cbonus * pst[:, p, P_CHARGES])
                shape[:, p] = np.where(lead, a.bomb_hit * pst[:, p, P_HITS] + a.coin * pst[:, p, P_MONEY]
                                       - a.lead_bomb_miss * pst[:, p, P_MISSES], -a.bomb_miss * pst[:, p, P_MISSES])
            quiet = np.where(info[:, I_DSCORE] + info[:, 8 + I_DSCORE] > 0, 0, quiet + 1)
            lull = (quiet >= quiet_steps)[:, None]
            own += np.where(lull, a.quiet_charge, 0.0) * pst[:, :, P_CHARGES] - a.change_cost * pst[:, :, P_WCHANGES]
            own += a.level * info[:, [I_LEVEL_DONE]]
            r = own + shape + a.team * own[:, ::-1] - np.where(role == 0, a.protect * harm[:, ::-1], 0.0)
            r = r.reshape(-1)
            dirs = acts[:, 0].cpu().numpy()
            r -= a.jitter * (dirs != prev_dir)
            r -= a.cycle_cost * acts[:, 3].cpu().numpy()
            prev_dir = dirs
            done = info[:, I_END] > 0
            done_a = np.repeat(done, 2)
            buf_rew[t] = r
            buf_done[t] = done_a
            ret_acc = ret_acc * a.gamma + r
            rew_std.update(ret_acc[learn_flat])
            ret_acc[done_a] = 0.0

            ep_acc["rew"] += r
            ep_acc["score"] += info[:, I_DSCORE] + info[:, 8 + I_DSCORE]
            ep_acc["deaths"] += info[:, [I_DEATHS, 8 + I_DEATHS]]
            ep_acc["pst"] += pst
            for e in np.nonzero(done)[0]:
                md = MODES[mode[e]]
                ls = [p for p in range(2) if learner[e, p]]
                ep_stats[md + "/score"].append(ep_acc["score"][e])
                ep_stats[md + "/levels"].append(info[e, I_LEVELS_DONE] + info[e, I_LEVEL_DONE] * 0)
                ep_stats[md + "/progress"].append(info[e, I_PROGRESS])
                ep_stats[md + "/ticks"].append(info[e, I_TICKS])
                ep_stats[md + "/learner_deaths"].append(np.mean([ep_acc["deaths"][e, p] for p in ls]))
                ep_stats[md + "/partner_deaths"].append(np.mean([ep_acc["deaths"][e, 1 - p] for p in ls]) if md != "duo" else np.mean(ep_acc["deaths"][e]))
                ep_stats[md + "/end"].append(info[e, I_END])
                ep_stats[md + "/start_level"].append(info[e, I_START_LEVEL])
                ep_stats["all/reward"].append(np.mean([ep_acc["rew"][e * 2 + p] for p in ls]))
                mins = max(info[e, I_TICKS], 1) / 1800
                for p in ls:
                    rn = ROLES[role[e, p]]
                    ps = ep_acc["pst"][e, p]
                    ep_stats[rn + "/charges_pm"].append(ps[P_CHARGES] / mins)
                    ep_stats[rn + "/wchanges_pm"].append(ps[P_WCHANGES] / mins)
                    ep_stats[rn + "/volleys_pm"].append(ps[P_VOLLEYS] / mins)
                    ep_stats[rn + "/misses_pm"].append(ps[P_MISSES] / mins)
                    ep_stats[rn + "/money_pm"].append(ps[P_MONEY] / mins)
                    ep_stats[rn + "/deaths"].append(ep_acc["deaths"][e, p])
                ep_acc["pst"][e] = 0
                ep_acc["score"][e] = 0
                ep_acc["deaths"][e] = 0
                ep_acc["rew"][e * 2:e * 2 + 2] = 0
                prev_dir[e * 2:e * 2 + 2] = 0
                quiet[e] = 0
                new_episode(e)
            total_steps += int(buf_valid[t].sum())

        t_roll = time.time() - t0
        # GAE.
        obs["self"] = obs["self"].copy()
        obs["self"][:, :, S_ROLE] = role
        with torch.no_grad():
            _, last_v = model(*to_dev(obs, dev))
        vals = buf_val.cpu().numpy()
        last_v = last_v.cpu().numpy()
        rs = buf_rew / rew_std.std
        adv = np.zeros((T, A), np.float32)
        gae = np.zeros(A, np.float32)
        for t in reversed(range(T)):
            nv = last_v if t == T - 1 else vals[t + 1]
            nd = 1.0 - buf_done[t]
            delta = rs[t] + a.gamma * nv * nd - vals[t]
            gae = delta + a.gamma * a.lam * nd * gae
            adv[t] = gae
        ret = adv + vals

        # PPO.
        model.train()
        valid = torch.from_numpy(buf_valid.reshape(-1)).to(dev)
        idx_all = torch.nonzero(valid).squeeze(1)
        b_ent = buf_ent.view(T * A, K, F)
        b_ent_id = buf_ent_id.view(T * A, K, 2)
        b_self = buf_self.view(T * A, S)
        b_self_id = buf_self_id.view(T * A, SI)
        b_act = buf_act.view(T * A, 4)
        b_mask = buf_mask.view(-1)
        b_logp = buf_logp.view(-1)
        b_adv = torch.from_numpy(adv.reshape(-1)).to(dev)
        b_ret = torch.from_numpy(ret.reshape(-1)).to(dev)
        b_val = buf_val.view(-1)
        stats = collections.defaultdict(list)
        N = idx_all.numel()
        mb = max(1, N // a.minibatches)
        for ep in range(a.epochs):
            perm = idx_all[torch.randperm(N, device=dev)]
            for i in range(0, N - mb + 1, mb):
                ix = perm[i:i + mb]
                logits, v = model(b_ent[ix], b_ent_id[ix], b_self[ix], b_self_id[ix])
                logp, ent = evaluate(logits, b_act[ix], model.acts, b_mask[ix])
                ratio = (logp - b_logp[ix]).exp()
                ad = b_adv[ix]
                ad = (ad - ad.mean()) / (ad.std() + 1e-8)
                pg = -torch.min(ratio * ad, ratio.clamp(1 - a.clip, 1 + a.clip) * ad).mean()
                vc = b_val[ix] + (v - b_val[ix]).clamp(-a.clip * 5, a.clip * 5)
                vl = 0.5 * torch.max((v - b_ret[ix]) ** 2, (vc - b_ret[ix]) ** 2).mean()
                ent_m = (ent * ent_heads).sum(-1).mean()
                loss = pg + a.vf * vl - a.ent * ent_m
                opt.zero_grad()
                loss.backward()
                torch.nn.utils.clip_grad_norm_(model.parameters(), 0.5)
                opt.step()
                with torch.no_grad():
                    stats["pg"].append(pg.item())
                    stats["vl"].append(vl.item())
                    stats["ent"].append(ent.mean(0).cpu().numpy())
                    stats["kl"].append(((ratio - 1) - (logp - b_logp[ix])).mean().item())
                    stats["clipfrac"].append(((ratio - 1).abs() > a.clip).float().mean().item())

        dt = time.time() - t0
        rec = {
            "update": update + 1,
            "steps": total_steps,
            "sps": round(N / dt),
            "t_env": round(t_env, 2),
            "t_roll": round(t_roll, 2),
            "t_total": round(dt, 2),
            "pg": float(np.mean(stats["pg"])),
            "vl": float(np.mean(stats["vl"])),
            "kl": float(np.mean(stats["kl"])),
            "clipfrac": float(np.mean(stats["clipfrac"])),
            "ent": [round(float(x), 3) for x in np.mean(stats["ent"], 0)],
            "rew_std": rew_std.std,
            "pool": len(pool),
            "charge_bonus": round(cbonus, 4),
        }
        for k, v in ep_stats.items():
            rec[k] = round(float(np.mean(v)), 3)
            rec[k.split("/")[0] + "/n"] = len(v)
        ep_stats.clear()
        log.write(json.dumps(rec) + "\n")
        log.flush()
        print(" ".join(f"{k}={v}" for k, v in rec.items() if not isinstance(v, list) or k == "ent"), flush=True)

        u = update + 1
        if u % a.snapshot_every == 0:
            pool.append((u, {k: v.detach().cpu().clone() for k, v in model.state_dict().items()}))
            if len(pool) > a.pool:
                # Keep the oldest few for weak partners, thin out the middle.
                del pool[1 + rng.integers(0, len(pool) - 2)]
        ck = {"model": model.state_dict(), "opt": opt.state_dict(), "update": u, "total_steps": total_steps,
              "cfg": model.cfg, "pool": pool}
        if u % 10 == 0:
            torch.save(ck, os.path.join(run_dir, "latest.pt"))
        if u % a.save_every == 0:
            torch.save({"model": model.state_dict(), "update": u, "total_steps": total_steps, "cfg": model.cfg},
                       os.path.join(run_dir, f"ckpt_{u:05d}.pt"))
            export(model, os.path.join(run_dir, f"policy_{u:05d}.bin"))
    env.close()


if __name__ == "__main__":
    main()
