"""The co-pilot policy: a small entity transformer.

Every on-screen object is a token (its sprite id embedded from two hash
buckets plus its float features); the player's own state is one more
token. Two pre-norm attention blocks mix them; the self token and a
max-pool over objects feed the action heads (direction, air weapon,
ground weapon, weapon select) and the value head.

The layers are written out by hand (no nn.MultiheadAttention) so the
Rust forward pass (src/rl/policy.rs) can mirror them exactly.
"""

import struct

import torch
import torch.nn as nn
import torch.nn.functional as Fn

ID_BUCKETS = 1024
HEADS = [9, 2, 2, 2]  # direction, air, ground, cycle


class Block(nn.Module):
    def __init__(self, d, heads, ff):
        super().__init__()
        self.h = heads
        self.ln1 = nn.LayerNorm(d)
        self.qkv = nn.Linear(d, 3 * d)
        self.proj = nn.Linear(d, d)
        self.ln2 = nn.LayerNorm(d)
        self.ff1 = nn.Linear(d, ff)
        self.ff2 = nn.Linear(ff, d)

    def forward(self, x, mask):
        # x [B, T, D]; mask [B, T] True = present
        B, T, D = x.shape
        h = self.ln1(x)
        q, k, v = self.qkv(h).split(D, dim=-1)
        dh = D // self.h
        q = q.view(B, T, self.h, dh).transpose(1, 2)
        k = k.view(B, T, self.h, dh).transpose(1, 2)
        v = v.view(B, T, self.h, dh).transpose(1, 2)
        # softmax(q k^T / sqrt(dh)) v over the present keys only.
        y = Fn.scaled_dot_product_attention(q, k, v, attn_mask=mask[:, None, None, :])
        y = y.transpose(1, 2).reshape(B, T, D)
        x = x + self.proj(y)
        x = x + self.ff2(Fn.relu(self.ff1(self.ln2(x))))
        return x


class Policy(nn.Module):
    def __init__(self, F, S, self_ids=3, d=96, heads=4, layers=2, emb=32, k=64):
        super().__init__()
        self.cfg = dict(F=F, S=S, self_ids=self_ids, d=d, heads=heads, layers=layers, emb=emb, k=k)
        self.k = k
        self.emb_a = nn.Embedding(ID_BUCKETS, emb)
        self.emb_b = nn.Embedding(ID_BUCKETS, emb)
        self.ent_in = nn.Linear(F + emb, d)
        self.ent_in2 = nn.Linear(d, d)
        self.self_in = nn.Linear(S + emb, d)
        self.self_in2 = nn.Linear(d, d)
        self.blocks = nn.ModuleList([Block(d, heads, 2 * d) for _ in range(layers)])
        self.ln_out = nn.LayerNorm(d)
        self.trunk = nn.Linear(2 * d, 256)
        self.pi = nn.Linear(256, sum(HEADS))
        self.v = nn.Linear(256, 1)
        nn.init.orthogonal_(self.pi.weight, 0.01)
        nn.init.zeros_(self.pi.bias)
        nn.init.orthogonal_(self.v.weight, 1.0)

    def embed(self, ids):
        # ids [..., 2]
        return self.emb_a(ids[..., 0]) + self.emb_b(ids[..., 1])

    def forward(self, ent, ent_id, selfv, self_id):
        """ent [B,K,F] ent_id [B,K,2] selfv [B,S] self_id [B,SELF_IDS*2].
        Returns (logits [B, sum(HEADS)], value [B])."""
        ent = ent[:, : self.k]
        ent_id = ent_id[:, : self.k]
        B = ent.shape[0]
        present = ent[..., -1] > 0.5
        e = torch.cat([ent, self.embed(ent_id)], -1)
        e = self.ent_in2(Fn.relu(self.ent_in(e)))
        wid = self_id.view(B, -1, 2)
        wemb = self.embed(wid).sum(1)
        s = torch.cat([selfv, wemb], -1)
        s = self.self_in2(Fn.relu(self.self_in(s)))
        x = torch.cat([s[:, None], e], 1)
        mask = torch.cat([torch.ones(B, 1, dtype=torch.bool, device=ent.device), present], 1)
        for b in self.blocks:
            x = b(x, mask)
        x = self.ln_out(x)
        pooled = x[:, 1:].masked_fill(~present[..., None], -1e9).max(1).values
        pooled = torch.where(present.any(1, keepdim=True), pooled, torch.zeros_like(pooled))
        h = Fn.relu(self.trunk(torch.cat([x[:, 0], pooled], -1)))
        return self.pi(h), self.v(h).squeeze(-1)


def split_logits(logits):
    return torch.split(logits, HEADS, dim=-1)


def sample(logits, temperature=1.0):
    """Returns (actions [B,4] long, logp [B])."""
    acts, logps = [], []
    for lg in split_logits(logits):
        d = torch.distributions.Categorical(logits=lg / temperature)
        a = d.sample()
        acts.append(a)
        logps.append(d.log_prob(a))
    return torch.stack(acts, -1), torch.stack(logps, -1).sum(-1)


def evaluate(logits, acts):
    logps, ents = [], []
    for i, lg in enumerate(split_logits(logits)):
        d = torch.distributions.Categorical(logits=lg)
        logps.append(d.log_prob(acts[:, i]))
        ents.append(d.entropy())
    return torch.stack(logps, -1).sum(-1), torch.stack(ents, -1)


def pack_actions(acts):
    """[B,4] (dir, air, ground, cycle) -> uint8 action bytes."""
    a = acts.to(torch.int64)
    return (a[:, 0] | (a[:, 1] << 4) | (a[:, 2] << 5) | (a[:, 3] << 6)).to(torch.uint8)


def export(model, path):
    """Writes the weights for src/rl/policy.rs: b"DRP1", a JSON config, then
    named f32 tensors."""
    import json

    sd = {k: v.detach().float().cpu().contiguous() for k, v in model.state_dict().items()}
    with open(path, "wb") as f:
        f.write(b"DRP1")
        cfg = json.dumps(model.cfg).encode()
        f.write(struct.pack("<I", len(cfg)) + cfg)
        f.write(struct.pack("<I", len(sd)))
        for name, t in sd.items():
            nb = name.encode()
            f.write(struct.pack("<I", len(nb)) + nb)
            f.write(struct.pack("<I", t.dim()))
            for s in t.shape:
                f.write(struct.pack("<I", s))
            f.write(t.numpy().astype("<f4").tobytes())
