//! The trained co-pilot network, run on the CPU: the forward pass of
//! tools/rl/model.py `Policy` over the weights `model.export` writes.
//! Objects that are not present are skipped outright, which is exact: in
//! the Python model they are masked out as attention keys and of the pool,
//! so they never influence the present tokens.

use std::collections::HashMap;

use super::obs::{Action, Obs, F, K, S, SELF_IDS};

/// Logits per action head: direction, air, ground, cycle.
pub const HEADS: [usize; 4] = [9, 2, 2, 2];

struct Linear {
    w: Vec<f32>, // [out][in]
    b: Vec<f32>,
    inp: usize,
    out: usize,
}

impl Linear {
    fn apply(&self, x: &[f32], y: &mut Vec<f32>) {
        y.clear();
        y.resize(self.out, 0.0);
        for o in 0..self.out {
            let row = &self.w[o * self.inp..(o + 1) * self.inp];
            let mut acc = self.b[o];
            for i in 0..self.inp {
                acc += row[i] * x[i];
            }
            y[o] = acc;
        }
    }
}

struct LayerNorm {
    g: Vec<f32>,
    b: Vec<f32>,
}

impl LayerNorm {
    fn apply(&self, x: &[f32], y: &mut Vec<f32>) {
        let n = x.len() as f32;
        let mean = x.iter().sum::<f32>() / n;
        let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
        let inv = 1.0 / (var + 1e-5).sqrt();
        y.clear();
        y.extend(x.iter().enumerate().map(|(i, v)| (v - mean) * inv * self.g[i] + self.b[i]));
    }
}

struct Block {
    ln1: LayerNorm,
    qkv: Linear,
    proj: Linear,
    ln2: LayerNorm,
    ff1: Linear,
    ff2: Linear,
}

pub struct Policy {
    /// Action head sizes: direction, air (2, or 3 with the charge order),
    /// ground, cycle.
    acts: [usize; 4],
    /// Self floats it was trained on (older policies: the first 48).
    s_in: usize,
    /// Observation version: 1 embedded the weapon slots by sprite alone.
    obs_v: usize,
    d: usize,
    heads: usize,
    emb: usize,
    k: usize,
    emb_a: Vec<f32>,
    emb_b: Vec<f32>,
    ent_in: Linear,
    ent_in2: Linear,
    self_in: Linear,
    self_in2: Linear,
    blocks: Vec<Block>,
    ln_out: LayerNorm,
    trunk: Linear,
    pi: Linear,
    v: Linear,
}

fn cfg_int(json: &str, key: &str) -> Option<usize> {
    let i = json.find(&format!("\"{key}\""))?;
    let rest = &json[i + key.len() + 2..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

impl Policy {
    pub fn load(path: &std::path::Path) -> Result<Policy, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Policy::from_bytes(&bytes)
    }

    pub fn from_bytes(b: &[u8]) -> Result<Policy, String> {
        let mut o = 0usize;
        let mut u32_at = |o: &mut usize| -> Result<u32, String> {
            let v = b.get(*o..*o + 4).ok_or("truncated")?;
            *o += 4;
            Ok(u32::from_le_bytes(v.try_into().unwrap()))
        };
        if b.get(0..4) != Some(b"DRP1") {
            return Err("not a co-pilot policy file".into());
        }
        o += 4;
        let n = u32_at(&mut o)? as usize;
        let cfg = String::from_utf8_lossy(b.get(o..o + n).ok_or("truncated")?).into_owned();
        o += n;
        let count = u32_at(&mut o)?;
        let mut t: HashMap<String, (Vec<usize>, Vec<f32>)> = HashMap::new();
        for _ in 0..count {
            let n = u32_at(&mut o)? as usize;
            let name = String::from_utf8_lossy(b.get(o..o + n).ok_or("truncated")?).into_owned();
            o += n;
            let nd = u32_at(&mut o)?;
            let mut shape = Vec::new();
            for _ in 0..nd {
                shape.push(u32_at(&mut o)? as usize);
            }
            let len: usize = shape.iter().product();
            let raw = b.get(o..o + len * 4).ok_or("truncated")?;
            o += len * 4;
            let data = raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            t.insert(name, (shape, data));
        }
        let get = |name: &str| -> Result<(Vec<usize>, Vec<f32>), String> { t.get(name).cloned().ok_or(format!("missing {name}")) };
        let lin = |name: &str| -> Result<Linear, String> {
            let (s, w) = get(&format!("{name}.weight"))?;
            let (_, bias) = get(&format!("{name}.bias"))?;
            Ok(Linear { w, b: bias, out: s[0], inp: s[1] })
        };
        let ln = |name: &str| -> Result<LayerNorm, String> {
            Ok(LayerNorm { g: get(&format!("{name}.weight"))?.1, b: get(&format!("{name}.bias"))?.1 })
        };
        let d = cfg_int(&cfg, "d").ok_or("config: d")?;
        let heads = cfg_int(&cfg, "heads").ok_or("config: heads")?;
        let layers = cfg_int(&cfg, "layers").ok_or("config: layers")?;
        let emb = cfg_int(&cfg, "emb").ok_or("config: emb")?;
        let k = cfg_int(&cfg, "k").ok_or("config: k")?.min(K);
        let s_in = cfg_int(&cfg, "S").unwrap_or(0);
        if cfg_int(&cfg, "F") != Some(F) || !(s_in == 48 || s_in == S) || cfg_int(&cfg, "self_ids") != Some(SELF_IDS) {
            return Err(format!("policy was trained for another observation layout: {cfg}"));
        }
        let air = cfg_int(&cfg, "air").unwrap_or(2);
        let obs_v = cfg_int(&cfg, "obs").unwrap_or(1);
        let mut blocks = Vec::new();
        for i in 0..layers {
            let p = format!("blocks.{i}");
            blocks.push(Block {
                ln1: ln(&format!("{p}.ln1"))?,
                qkv: lin(&format!("{p}.qkv"))?,
                proj: lin(&format!("{p}.proj"))?,
                ln2: ln(&format!("{p}.ln2"))?,
                ff1: lin(&format!("{p}.ff1"))?,
                ff2: lin(&format!("{p}.ff2"))?,
            });
        }
        Ok(Policy {
            acts: [HEADS[0], air, HEADS[2], HEADS[3]],
            s_in,
            obs_v,
            d,
            heads,
            emb,
            k,
            emb_a: get("emb_a.weight")?.1,
            emb_b: get("emb_b.weight")?.1,
            ent_in: lin("ent_in")?,
            ent_in2: lin("ent_in2")?,
            self_in: lin("self_in")?,
            self_in2: lin("self_in2")?,
            blocks,
            ln_out: ln("ln_out")?,
            trunk: lin("trunk")?,
            pi: lin("pi")?,
            v: lin("v")?,
        })
    }

    fn add_embed(&self, a: i32, b: i32, out: &mut [f32]) {
        let e = self.emb;
        let (a, b) = (a.max(0) as usize, b.max(0) as usize);
        for i in 0..e {
            out[i] += self.emb_a[a * e + i] + self.emb_b[b * e + i];
        }
    }

    /// Logits (all heads concatenated) and the value.
    pub fn forward(&self, o: &Obs) -> (Vec<f32>, f32) {
        let d = self.d;
        let mut tokens: Vec<Vec<f32>> = Vec::new();
        let mut tmp = Vec::new();
        let mut tmp2 = Vec::new();
        // The self token.
        let si = self.s_in;
        let mut sin = vec![0.0; si + self.emb];
        sin[..si].copy_from_slice(&o.selfv[..si]);
        let ids = if self.obs_v >= 2 { &o.self_id } else { &o.self_id_v1 };
        if self.obs_v < 2 {
            // Version 1 scaled the weapon slots' frames by 1/100.
            for x in sin[15..18].iter_mut() {
                *x *= 4.0 / 100.0;
            }
        }
        for w in 0..SELF_IDS {
            self.add_embed(ids[w * 2], ids[w * 2 + 1], &mut sin[si..]);
        }
        self.self_in.apply(&sin, &mut tmp);
        tmp.iter_mut().for_each(|v| *v = v.max(0.0));
        self.self_in2.apply(&tmp, &mut tmp2);
        tokens.push(tmp2.clone());
        // Objects.
        let mut ein = vec![0.0; F + self.emb];
        for i in 0..self.k {
            let f = &o.ent[i * F..(i + 1) * F];
            if f[F - 1] <= 0.5 {
                continue;
            }
            ein[..F].copy_from_slice(f);
            ein[F..].iter_mut().for_each(|v| *v = 0.0);
            self.add_embed(o.ent_id[i * 2], o.ent_id[i * 2 + 1], &mut ein[F..]);
            self.ent_in.apply(&ein, &mut tmp);
            tmp.iter_mut().for_each(|v| *v = v.max(0.0));
            self.ent_in2.apply(&tmp, &mut tmp2);
            tokens.push(tmp2.clone());
        }
        let t = tokens.len();
        let dh = d / self.heads;
        let scale = 1.0 / (dh as f32).sqrt();
        for b in &self.blocks {
            let mut qkv: Vec<Vec<f32>> = Vec::with_capacity(t);
            for x in &tokens {
                b.ln1.apply(x, &mut tmp);
                let mut y = Vec::new();
                b.qkv.apply(&tmp, &mut y);
                qkv.push(y);
            }
            let mut att = vec![0.0f32; t];
            let mut ycat = vec![0.0f32; d];
            for i in 0..t {
                for h in 0..self.heads {
                    let q = &qkv[i][h * dh..(h + 1) * dh];
                    let mut mx = f32::NEG_INFINITY;
                    for j in 0..t {
                        let k = &qkv[j][d + h * dh..d + (h + 1) * dh];
                        let s: f32 = q.iter().zip(k).map(|(a, b)| a * b).sum::<f32>() * scale;
                        att[j] = s;
                        mx = mx.max(s);
                    }
                    let mut sum = 0.0;
                    for j in 0..t {
                        att[j] = (att[j] - mx).exp();
                        sum += att[j];
                    }
                    let out = &mut ycat[h * dh..(h + 1) * dh];
                    out.iter_mut().for_each(|v| *v = 0.0);
                    for j in 0..t {
                        let w = att[j] / sum;
                        let v = &qkv[j][2 * d + h * dh..2 * d + (h + 1) * dh];
                        for c in 0..dh {
                            out[c] += w * v[c];
                        }
                    }
                }
                b.proj.apply(&ycat, &mut tmp);
                for c in 0..d {
                    tokens[i][c] += tmp[c];
                }
            }
            for x in tokens.iter_mut() {
                b.ln2.apply(x, &mut tmp);
                b.ff1.apply(&tmp, &mut tmp2);
                tmp2.iter_mut().for_each(|v| *v = v.max(0.0));
                b.ff2.apply(&tmp2, &mut tmp);
                for c in 0..d {
                    x[c] += tmp[c];
                }
            }
        }
        let mut normed: Vec<Vec<f32>> = Vec::with_capacity(t);
        for x in &tokens {
            self.ln_out.apply(x, &mut tmp);
            normed.push(tmp.clone());
        }
        let mut cat = vec![0.0f32; 2 * d];
        cat[..d].copy_from_slice(&normed[0]);
        if t > 1 {
            for c in 0..d {
                cat[d + c] = normed[1..].iter().map(|x| x[c]).fold(f32::NEG_INFINITY, f32::max);
            }
        }
        self.trunk.apply(&cat, &mut tmp);
        tmp.iter_mut().for_each(|v| *v = v.max(0.0));
        let mut logits = Vec::new();
        self.pi.apply(&tmp, &mut logits);
        let mut v = Vec::new();
        self.v.apply(&tmp, &mut v);
        (logits, v[0])
    }

    /// Samples an action at `temperature` (1 = as trained; lower is more
    /// decisive, higher more erratic). `r` gives uniform numbers in [0, 1).
    pub fn act(&self, o: &Obs, temperature: f32, r: &mut impl FnMut() -> f32) -> Action {
        let (logits, _) = self.forward(o);
        let mut picks = [0usize; 4];
        let mut off = 0;
        for (h, &n) in self.acts.iter().enumerate() {
            let lg = &logits[off..off + n];
            off += n;
            let t = temperature.max(1e-3);
            let mx = lg.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let p: Vec<f32> = lg.iter().map(|v| ((v - mx) / t).exp()).collect();
            let sum: f32 = p.iter().sum();
            let mut u = r() * sum;
            picks[h] = n - 1;
            for (i, pi) in p.iter().enumerate() {
                if u < *pi {
                    picks[h] = i;
                    break;
                }
                u -= pi;
            }
        }
        Action { dir: picks[0] as u8, air: picks[1] == 1, ground: picks[2] == 1, cycle: picks[3] == 1, charge: picks[1] == 2 }
    }
}
