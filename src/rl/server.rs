//! `deimos --rl-server`: many training games stepped in parallel, driven
//! over stdin/stdout by the trainer (tools/rl).
//!
//! Options: `--envs N` (games), `--threads T`, `--repeat R` (ticks per
//! decision), `--max-ticks N`, `--max-levels N`, `--levels a,b,..`
//! (starting levels), `--seed S`, `--obs-delay D` (decisions the
//! observation lags, default 1).
//!
//! Protocol (little-endian). On start the server writes the header
//! `b"DRL2"`, then u32 N, K, F, S, SELF_IDS, INFO, then an observation
//! block. Then, per command byte read:
//! - `S` + N*2 action bytes (env-major, player-minor; bits 0..3 direction,
//!   4 air, 5 ground, 6 cycle, 7 charge) + N*2 flag bytes (bit 0: this
//!   player is a learner): steps every game and writes an observation
//!   block.
//! - `Q`: exits.
//!
//! An observation block is f32 ent[N][2][K][F], i32 ent_id[N][2][K][2],
//! f32 self[N][2][S], i32 self_id[N][2][SELF_IDS*2], f32 info[N][INFO].
//! Games that ended were reset already; their info carries the final
//! step's numbers with info[19] (end reason) > 0.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use super::env::{Env, EnvConfig, INFO};
use super::obs::{Action, F, K, S, SELF_IDS};

fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[derive(Default)]
struct Chunk {
    ent: Vec<u8>,
    ent_id: Vec<u8>,
    selfv: Vec<u8>,
    self_id: Vec<u8>,
    info: Vec<u8>,
}

fn put_f32(out: &mut Vec<u8>, v: &[f32]) {
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}
fn put_i32(out: &mut Vec<u8>, v: &[i32]) {
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

fn write_chunk(envs: &[Env]) -> Chunk {
    let mut c = Chunk::default();
    for e in envs {
        for p in 0..2 {
            put_f32(&mut c.ent, &e.obs[p].ent);
            put_i32(&mut c.ent_id, &e.obs[p].ent_id);
            put_f32(&mut c.selfv, &e.obs[p].selfv);
            put_i32(&mut c.self_id, &e.obs[p].self_id);
        }
        put_f32(&mut c.info, &e.info);
    }
    c
}

pub fn levels_arg(args: &[String], n_levels: i32) -> Vec<i32> {
    let v: Vec<i32> = args
        .iter()
        .position(|a| a == "--levels")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_default();
    if v.is_empty() {
        (1..=n_levels).collect()
    } else {
        v
    }
}

pub fn run(args: &[String], exe: Vec<u8>, paks: PathBuf) {
    let n: usize = arg(args, "--envs", 64);
    let threads: usize = arg(args, "--threads", 16).clamp(1, n);
    let seed: u64 = arg(args, "--seed", 1);
    let repeat: u32 = arg(args, "--repeat", 3);
    let max_ticks: u32 = arg(args, "--max-ticks", 15000);
    let max_levels: i32 = arg(args, "--max-levels", 2);
    let delay: usize = arg(args, "--obs-delay", 1);
    let exe = Arc::new(exe);
    let args = Arc::new(args.to_vec());

    let mut senders = Vec::new();
    let mut receivers = Vec::new();
    let mut start = 0;
    for t in 0..threads {
        let count = n / threads + (t < n % threads) as usize;
        let range = start..start + count;
        start += count;
        let (tx_cmd, rx_cmd) = mpsc::channel::<Option<Arc<Vec<u8>>>>();
        let (tx_out, rx_out) = mpsc::channel::<Chunk>();
        let exe = exe.clone();
        let paks = paks.clone();
        let args = args.clone();
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn(move || {
                let mut envs: Vec<Env> = Vec::new();
                for i in range.clone() {
                    envs.push(Env::new(&exe, &paks, seed.wrapping_mul(1000003).wrapping_add(i as u64 + 1)).expect("env"));
                }
                let n_levels = crate::game::og::level::G_Level_GetNumInLevelList(&mut envs[0].s.g);
                let cfg = EnvConfig { repeat, max_ticks, max_levels, levels: levels_arg(&args, n_levels), delay };
                for e in envs.iter_mut() {
                    e.reset(&cfg).expect("reset");
                }
                tx_out.send(write_chunk(&envs)).unwrap();
                while let Ok(Some(acts)) = rx_cmd.recv() {
                    for (j, e) in envs.iter_mut().enumerate() {
                        let i = range.start + j;
                        let a0 = acts[i * 2];
                        let a1 = acts[i * 2 + 1];
                        let learners = [acts[n * 2 + i * 2] & 1 != 0, acts[n * 2 + i * 2 + 1] & 1 != 0];
                        e.step(&cfg, [Action::from_byte(a0), Action::from_byte(a1)], learners).expect("step");
                    }
                    tx_out.send(write_chunk(&envs)).unwrap();
                }
            })
            .unwrap();
        senders.push(tx_cmd);
        receivers.push(rx_out);
    }

    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::with_capacity(8 << 20, stdout.lock());
    out.write_all(b"DRL2").unwrap();
    for v in [n, K, F, S, SELF_IDS, INFO] {
        out.write_all(&(v as u32).to_le_bytes()).unwrap();
    }
    let emit = |out: &mut std::io::BufWriter<std::io::StdoutLock>, chunks: Vec<Chunk>| {
        for c in &chunks {
            out.write_all(&c.ent).unwrap();
        }
        for c in &chunks {
            out.write_all(&c.ent_id).unwrap();
        }
        for c in &chunks {
            out.write_all(&c.selfv).unwrap();
        }
        for c in &chunks {
            out.write_all(&c.self_id).unwrap();
        }
        for c in &chunks {
            out.write_all(&c.info).unwrap();
        }
        out.flush().unwrap();
    };
    let chunks: Vec<Chunk> = receivers.iter().map(|r| r.recv().unwrap()).collect();
    emit(&mut out, chunks);

    let mut stdin = std::io::stdin().lock();
    let mut cmd = [0u8; 1];
    loop {
        if stdin.read_exact(&mut cmd).is_err() {
            break;
        }
        match cmd[0] {
            b'S' => {
                let mut acts = vec![0u8; n * 4];
                stdin.read_exact(&mut acts).unwrap();
                let acts = Arc::new(acts);
                for s in &senders {
                    s.send(Some(acts.clone())).unwrap();
                }
                let chunks: Vec<Chunk> = receivers.iter().map(|r| r.recv().unwrap()).collect();
                emit(&mut out, chunks);
            }
            _ => break,
        }
    }
    for s in &senders {
        let _ = s.send(None);
    }
}

/// `deimos --rl-bench [--envs N --threads T --steps M]`: steps random
/// actions and reports the speed.
pub fn bench(args: &[String], exe: Vec<u8>, paks: PathBuf) {
    let steps: u32 = arg(args, "--steps", 2000);
    let repeat: u32 = arg(args, "--repeat", 3);
    let t0 = std::time::Instant::now();
    let mut e = Env::new(&exe, &paks, 7).expect("env");
    let n_levels = crate::game::og::level::G_Level_GetNumInLevelList(&mut e.s.g);
    let cfg = EnvConfig { repeat, max_ticks: 15000, max_levels: 2, levels: levels_arg(args, n_levels), delay: 1 };
    e.reset(&cfg).unwrap();
    eprintln!("startup {:?}, levels in list: {n_levels}", t0.elapsed());
    let t1 = std::time::Instant::now();
    let mut r: u32 = 1;
    let mut eps = 0;
    let mut max_ent = 0;
    let mut stats = [0f32; INFO - 25];
    for _ in 0..steps {
        r = r.wrapping_mul(1103515245).wrapping_add(12345);
        let a0 = Action::from_byte(((r >> 16) as u8 % 9) | 16 | 32 | if (r >> 24) % 16 == 0 { 128 } else { 0 });
        let a1 = Action::from_byte(((r >> 20) as u8 % 9) | 16);
        if std::env::var_os("RL_DEBUG").is_some() {
            let p = e.s.player(0);
            eprintln!("t={} st={} lives={} sh={} in={} pos=({},{})", e.s.game_time(), p.state, p.lives, p.shields_pct, p.in_game, p.x, p.y);
        }
        if e.step(&cfg, [a0, a1], [true, true]).unwrap() {
            eps += 1;
            eprintln!("episode end: {:?}", &e.info[..]);
        }
        for (k, v) in stats.iter_mut().enumerate() {
            *v += e.info[25 + k];
        }
        let n_ent = (e.obs[0].selfv[46] * K as f32).round() as usize;
        max_ent = max_ent.max(n_ent);
    }
    let dt = t1.elapsed().as_secs_f64();
    eprintln!(
        "{steps} steps x {repeat} ticks in {dt:.2}s: {:.0} ticks/s, {eps} episodes, max entities {max_ent}, heap {} KiB",
        steps as f64 * repeat as f64 / dt,
        e.s.g.m.heap_used() / 1024
    );
    eprintln!("per player [volleys, hits, misses, charges, weapon changes, money]: {:?}", stats);
}

/// `deimos --rl-parity <policy.bin> <dump>`: compares this forward pass
/// with the logits tools/rl/parity.py recorded for the same observations.
pub fn parity(args: &[String]) {
    use super::obs::Obs;
    let i = args.iter().position(|a| a == "--rl-parity").unwrap();
    let pol = super::policy::Policy::load(std::path::Path::new(&args[i + 1])).expect("policy");
    let raw = std::fs::read(&args[i + 2]).expect("dump");
    let rd_f = |o: &mut usize, n: usize| -> Vec<f32> {
        let v = raw[*o..*o + n * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        *o += n * 4;
        v
    };
    let rd_i = |o: &mut usize, n: usize| -> Vec<i32> {
        let v = raw[*o..*o + n * 4].chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect();
        *o += n * 4;
        v
    };
    let mut o = 0;
    let n = rd_i(&mut o, 1)[0] as usize;
    let nl = rd_i(&mut o, 1)[0] as usize;
    let mut worst = 0f32;
    for _ in 0..n {
        let ob = Obs {
            ent: rd_f(&mut o, K * F),
            ent_id: rd_i(&mut o, K * 2),
            selfv: rd_f(&mut o, S),
            self_id: rd_i(&mut o, SELF_IDS * 2),
            ..Obs::default()
        };
        let want = rd_f(&mut o, nl + 1);
        let (lg, v) = pol.forward(&ob);
        for j in 0..nl {
            worst = worst.max((lg[j] - want[j]).abs());
        }
        worst = worst.max((v - want[nl]).abs());
    }
    println!("{n} samples, max abs difference {worst:.2e}");
}
