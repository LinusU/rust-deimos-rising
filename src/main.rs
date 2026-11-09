mod assets;
mod data;
mod game;
mod pak;

use pak::{id, PakSet, ResId, Tag, NONE_ID};
use std::path::PathBuf;

fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("DEIMOS_DATA") {
        return PathBuf::from(d);
    }
    for cand in [
        PathBuf::from("retail/ Data/Paks"),
        PathBuf::from("../retail/ Data/Paks"),
    ] {
        if cand.is_dir() {
            return cand;
        }
    }
    PathBuf::from("retail/ Data/Paks")
}

fn dump_data() {
    let dir = data_dir();
    println!("Opening paks in {:?}", dir);
    let mut set = PakSet::open_dir(&dir).expect("open paks");
    for (path, pak) in &set.paks {
        println!("  {:?}: {} entries", path.file_name().unwrap(), pak.entries_of_tag(Tag::Im08).len() + pak.entries_of_tag(Tag::Im16).len() + pak.entries_of_tag(Tag::Soun).len() + pak.entries_of_tag(Tag::Unde).len() + pak.entries_of_tag(Tag::Leve).len() + pak.entries_of_tag(Tag::Wede).len() + pak.entries_of_tag(Tag::Plde).len() + pak.entries_of_tag(Tag::Flli).len() + pak.entries_of_tag(Tag::Reli).len() + pak.entries_of_tag(Tag::Edli).len() + pak.entries_of_tag(Tag::Coli).len() + pak.entries_of_tag(Tag::Tefo).len() + pak.entries_of_tag(Tag::Stli).len() + pak.entries_of_tag(Tag::Film).len() + pak.entries_of_tag(Tag::Pref).len());
    }

    // Parse every text-format file and report failures.
    let mut counts: Vec<(Tag, usize)> = Vec::new();
    let mut failures = 0usize;
    for tag in [
        Tag::Unde, Tag::Leve, Tag::Plde, Tag::Wede, Tag::Flli, Tag::Reli, Tag::Edli, Tag::Coli,
        Tag::Tefo, Tag::Stli, Tag::Pref,
    ] {
        let rids: Vec<pak::ResId> = set.entries_of_tag(tag).iter().map(|e| e.rid).collect();
        let mut ok = 0usize;
        for rid in rids {
            match tag {
                Tag::Unde => {
                    if let Some(t) = set.read_text(tag, rid) {
                        let d = data::UnitDef::parse(rid, &t);
                        if d.name.is_empty() {
                            println!("  UNDE {:?} -> empty name", rid);
                            failures += 1;
                        } else {
                            ok += 1;
                        }
                    }
                }
                Tag::Leve => {
                    if let Some(t) = set.read_text(tag, rid) {
                        let d = data::LevelDef::parse(&t);
                        if d.name.is_empty() {
                            println!("  LEVE {:?} -> empty name", rid);
                            failures += 1;
                        } else {
                            ok += 1;
                        }
                    }
                }
                Tag::Plde => {
                    if let Some(t) = set.read_text(tag, rid) {
                        let d = data::PlayerDef::parse(&t);
                        if d.name.is_empty() {
                            println!("  PLDE {:?} -> empty name", rid);
                            failures += 1;
                        } else {
                            ok += 1;
                        }
                    }
                }
                Tag::Wede => {
                    if let Some(t) = set.read_text(tag, rid) {
                        let d = data::WeaponDef::parse(rid, &t);
                        if d.name.is_empty() {
                            println!("  WEDE {:?} -> empty name", rid);
                            failures += 1;
                        } else {
                            ok += 1;
                        }
                    }
                }
                _ => {
                    if set.read_text(tag, rid).is_some() {
                        ok += 1;
                    }
                }
            }
        }
        counts.push((tag, ok));
    }
    for (tag, ok) in &counts {
        println!("  {:?}: {} parsed", tag, ok);
    }
    println!("failures: {}", failures);

    // Spot-check specific resources.
    let bagb = set.read_text(Tag::Unde, id("bagb")).unwrap();
    let d = data::UnitDef::parse(id("bagb"), &bagb);
    println!(
        "bagb: name={:?} states={} state0={:?} sprite={:?} dirs={} rules={}",
        d.name,
        d.states.len(),
        d.states[0].name,
        String::from_utf8_lossy(&d.states[0].sprite_face.to_le_bytes()),
        d.states[0].num_directions,
        d.states[0].rules.len()
    );

    let le01 = set.read_text(Tag::Leve, id("le01")).unwrap();
    let l = data::LevelDef::parse(&le01);
    println!(
        "le01: name={:?} bg={:?} objects={} music={:?} bgimg={:?}",
        l.name,
        l.background,
        l.objects.len(),
        String::from_utf8_lossy(&l.music.to_le_bytes()),
        String::from_utf8_lossy(&l.background_image.to_le_bytes())
    );

    let gafl = set.read_text(Tag::Flli, id("gafl")).unwrap();
    let f = data::FillList::parse(&gafl);
    println!(
        "gafl: {} floats, [0x34]={} [0x35]={} named(WepSelector_FadeOutRate)={:?}",
        f.entries.len(),
        f.get(0x34),
        f.get(0x35),
        f.named("WepSelector_FadeOutRate")
    );

    let pl01 = set.read_text(Tag::Plde, id("pl01")).unwrap();
    let p = data::PlayerDef::parse(&pl01);
    println!(
        "pl01: name={:?} lives={} scorebar={:?} entry=({},{})",
        p.name, p.life_num_initial,
        String::from_utf8_lossy(&p.sprite_score_bar.to_le_bytes()),
        p.entry_solo_start_x, p.entry_solo_start_y
    );

    // Check a GIF reads back.
    let gif = set.read_raw(Tag::Im08, id("BAGU")).unwrap();
    println!("im08 BAGU: {} bytes, magic={:?}", gif.len(), &gif[..6]);
    let _ = NONE_ID;
}

/// List the permanent list resources (flli/reli/coli/edli/stli) with names.
fn film_probe() {
    let dir = data_dir();
    let mut set = PakSet::open_dir(&dir).expect("open paks");
    let entries: Vec<_> = set.entries_of_tag(Tag::Film).into_iter().map(|e| (e.rid, e.display.clone())).collect();
    for (rid, display) in entries {
        let Some(data) = set.read_raw(Tag::Film, rid) else { continue };
        println!("=== {} {:08x} ({} bytes)", display, rid, data.len());
        if data.len() < 16 {
            continue;
        }
        let u32be = |o: usize| u32::from_be_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
        let u32le = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
        // Version 0x2715 detects endianness (decomp SwapULong fallback).
        let le_file = u32le(0) == 0x2715;
        let g = |o: usize| if le_file { u32le(o) } else { u32be(o) };
        println!("  version={:#x} seed={:#x} level={:08x} gameType={}", g(0), g(4), g(8), g(12));
        for p in 0..2 {
            let base = 16 + p * 0x4eac;
            if base + 12 > data.len() {
                break;
            }
            let frames = g(base);
            let field1 = g(base + 4);
            let field2 = g(base + 8);
            let input = &data[base + 12..(base + 12 + 0x4ea0).min(data.len())];
            let n = (frames as usize).min(input.len());
            // Per-bit on counts + pairwise co-occurrence.
            let mut on = [0usize; 7];
            let mut both = [[0usize; 7]; 7];
            for &b in input.iter().take(n) {
                for i in 0..7 {
                    if b & (1 << i) != 0 {
                        on[i] += 1;
                        for j in 0..7 {
                            if i != j && b & (1 << j) != 0 {
                                both[i][j] += 1;
                            }
                        }
                    }
                }
            }
            println!(
                "  p{}: frames={} field1={:#x} field2={:08x} bits_on={:?}",
                p, frames, field1, field2, on
            );
            // Print co-occurrence matrix compactly.
            for i in 0..7 {
                let row: Vec<String> = (0..7)
                    .map(|j| format!("{:>5}", if i == j { "-".to_string() } else { both[i][j].to_string() }))
                    .collect();
                println!("    bit{}: {}", i, row.join(" "));
            }
        }
    }
}

/// Brute-force the retail film bit→action mapping using the score oracle:
/// field1 records score+0xb3ac2 at the last frame, and the sim is
/// deterministic given the film seed.
fn film_test() {
    use game::sim::{PlayerInput, Sim, SlotInput};
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    // --filmtest <rid-hex> [--map b0,b1,...,b6]
    let args: Vec<String> = std::env::args().collect();
    let pos = args.iter().position(|x| x == "--filmtest").unwrap();
    let film_rid = u32::from_str_radix(&args[pos + 1], 16).unwrap();
    let film = a.film(film_rid).expect("film");
    println!(
        "film {:08x}: level={:08x} seed={:#x} gt={} frames={:?} score_marks={:?}",
        film_rid, film.level_id, film.seed, film.game_type, film.frames, film.score_marks
    );
    // Debug single replay: --filmedbg (with --filmtest) shows per-10s status.
    if args.iter().any(|a| a == "--filmedbg") {
        let film = a.film(film_rid).expect("film");
        let players = if film.frames[1] > 0 { 2 } else { 1 };
        // Recovered mapping (G_Film bit -> input byte -> action):
        // bit0=left bit1=right bit2=up bit3=down bit4=bomb bit5=air-fire bit6=cycle.
        let map = [2usize, 3, 0, 1, 5, 4, 6];
        let mut sim = game::render::start_level(&mut a, film.level_id, players, film.seed as u64).expect("level");
        let n = film.frames[0].min(20000);
        let mut fire_frames = 0u32;
        let mut bomb_frames = 0u32;
        for t in 0..n {
            let mk = |p: usize| -> SlotInput {
                let b = film.inputs[p].get(t).copied().unwrap_or(0);
                let mut s = SlotInput::default();
                for (bit, &f) in map.iter().enumerate() {
                    if b & (1 << bit) != 0 {
                        match f {
                            0 => s.up = true, 1 => s.down = true, 2 => s.left = true,
                            3 => s.right = true, 4 => s.fire = true, 5 => s.bomb = true,
                            _ => s.cycle = true,
                        }
                    }
                }
                s
            };
            let inp = PlayerInput { p1: mk(0), p2: mk(1) };
            if inp.p1.fire { fire_frames += 1; }
            if inp.p1.bomb { bomb_frames += 1; }
            let prev_phase = sim.players[0].as_ref().map(|p| p.phase);
            sim.advance(&a, &inp, 1.0 / 60.0);
            if let (Some(pp), Some(np)) = (prev_phase, sim.players[0].as_ref().map(|p| p.phase)) {
                if pp != np {
                    let near: Vec<String> = sim.entities.iter()
                        .filter(|e| !e.dead)
                        .take(6)
                        .map(|e| format!("{:08x}@({:.0},{:.0})", e.unit, e.pos.x, e.pos.y))
                        .collect();
                    println!("t={:>5} phase {:?} -> {:?} scr=({:.0},{:.0}) ents={}",
                        t, pp, np,
                        sim.players[0].as_ref().unwrap().screen.x,
                        sim.players[0].as_ref().unwrap().screen.y,
                        near.join(" "));
                }
            }
            if t % 600 == 599 {
                let p = sim.players[0].as_ref().unwrap();
                let mut projs = 0usize;
                let mut pinfo = String::new();
                for e in &sim.entities {
                    if let Some(d) = a.units.get(&e.unit) {
                        if d.player_projectile {
                            projs += 1;
                            if projs <= 3 {
                                pinfo.push_str(&format!(" [{:.0},{:.0} v{:.1},{:.1} tv{:.1},{:.1} st{}]",
                                    e.pos.x, e.pos.y, e.vel.x, e.vel.y, e.tvel.x, e.tvel.y, e.state));
                            }
                        }
                    }
                }
                println!("t={:>5} ph={:?} scr=({:.0},{:.0}) vel=({:.1},{:.1}) sh={:.0} lives={} score={} hit={} kill={}/{} shots={} proj={} ftmr={} bt={} ent={} pick={} coin={} pts={} money={} mult={}",
                    t, p.phase, p.screen.x, p.screen.y, p.vel.x, p.vel.y, p.shields, p.lives, p.score, sim.stat_hits, sim.stat_kills_player, sim.stat_kills, sim.stat_shots, projs, p.fire_timer, p.bomb_burst, sim.entities.len(), sim.stat_pickups, sim.stat_coins, sim.stat_score_points, p.money, p.multiplier);
                println!("   proj{}", pinfo);
            }
        }
        println!("fire_frames={fire_frames} bomb_frames={bomb_frames} / {n}");
        println!(
            "final: scroll={:.0}/{:.0} complete={} phase={:?} score={} money={} spawns={} endseq={}",
            sim.scroll, sim.max_scroll(), sim.level_complete,
            sim.players[0].as_ref().map(|p| p.phase),
            sim.players[0].as_ref().map(|p| p.score).unwrap_or(0),
            sim.players[0].as_ref().map(|p| p.money).unwrap_or(0),
            sim.stat_spawns,
            sim.end_seq_state,
        );
        return;
    }
    let players = if film.frames[1] > 0 { 2 } else { 1 };

    // Field ids: 0=up 1=down 2=left 3=right 4=fire 5=bomb 6=cycle.
    // bits {0,1} and {2,3} are the two axis pairs; bits {4,5,6} are the
    // action buttons in some order. Try every axis/order/action assignment.
    //
    // NOTE: the true retail mapping is recovered from the decomp —
    // G_Film::GetInputs permutes the byte into m_unk0..6 (= inputs[0..6])
    // and G_Player/G_WeaponHandler consume inputs[0]=up [1]=right
    // [2]=down [3]=left [4]=bomb(ground) [5]=air-fire [6]=cycle, giving
    // map=[2,3,0,1,5,4,6] (bit0=left bit1=right bit2=up bit3=down
    // bit4=bomb bit5=air-fire bit6=cycle). Earlier notes had bits 4/5
    // swapped — the WeaponHandler Process args are (inputs[4]=inA=ground,
    // inputs[5]=inB=air). Exact score replay still fails because
    // rand()-stream parity needs every call site ordered like the
    // original, so this search remains a diagnostic, not an oracle.
    let perms = [
        [4usize, 5, 6],
        [4, 6, 5],
        [5, 4, 6],
        [5, 6, 4],
        [6, 4, 5],
        [6, 5, 4],
    ];
    let mut cands: Vec<[usize; 7]> = Vec::new();
    for swap_axes in 0..2 {
        for s0 in 0..2 {
            for s1 in 0..2 {
                let (ax, ay) = if swap_axes == 0 {
                    ((0usize, 1usize), (2usize, 3usize))
                } else {
                    ((2usize, 3usize), (0usize, 1usize))
                };
                let mut map = [usize::MAX; 7];
                map[0] = if s0 == 0 { ax.0 } else { ax.1 };
                map[1] = if s0 == 0 { ax.1 } else { ax.0 };
                map[2] = if s1 == 0 { ay.0 } else { ay.1 };
                map[3] = if s1 == 0 { ay.1 } else { ay.0 };
                for perm in perms {
                    let mut m = map;
                    m[4] = perm[0];
                    m[5] = perm[1];
                    m[6] = perm[2];
                    cands.push(m);
                }
            }
        }
    }
    let mut best: Vec<(i64, i64, [usize; 7])> = Vec::new();
    for map in cands {
        // Replay.
        let mut sim = game::render::start_level(&mut a, film.level_id, players, film.seed as u64)
            .expect("level");
        let n = film.frames[0].min(20000);
        for t in 0..n {
            let mk = |p: usize| -> SlotInput {
                let b = film.inputs[p].get(t).copied().unwrap_or(0);
                let mut s = SlotInput::default();
                for (bit, &f) in map.iter().enumerate() {
                    if b & (1 << bit) != 0 {
                        match f {
                            0 => s.up = true,
                            1 => s.down = true,
                            2 => s.left = true,
                            3 => s.right = true,
                            4 => s.fire = true,
                            5 => s.bomb = true,
                            _ => s.cycle = true,
                        }
                    }
                }
                s
            };
            let inp = PlayerInput { p1: mk(0), p2: mk(1) };
            sim.advance(&a, &inp, 1.0 / 60.0);
        }
        let score = sim.players[0].as_ref().map(|p| p.score).unwrap_or(-1);
        let alive = sim.players[0]
            .as_ref()
            .map(|p| p.lives)
            .unwrap_or(0);
        best.push((score, alive, map));
    }
    best.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (s, l, m) in best.iter().take(10) {
        println!("score={:>8} lives={} diff={:>8} map={:?}", s, l, (s - film.score_marks[0]).abs(), m);
    }
    println!("expected score_marks={:?}", film.score_marks);
}

fn perm_probe() {
    let dir = data_dir();
    let mut set = PakSet::open_dir(&dir).expect("open paks");
    let first = set
        .entries_of_tag(Tag::Flli)
        .first()
        .map(|e| e.rid);
    if let Some(t) = set
        .read_text(Tag::Flli, pak::id("flli"))
        .or_else(|| first.and_then(|r| set.read_text(Tag::Flli, r)))
    {
        let fl = crate::data::FillList::parse(&t);
        for i in 0..fl.entries.len() {
            println!("pf({:#04x}) = {} ({})", i, fl.entries[i].1, fl.entries[i].0);
        }
    }
    for tag in [
        Tag::Flli, Tag::Reli, Tag::Coli, Tag::Edli, Tag::Stli, Tag::Tefo, Tag::Film, Tag::Pref,
        Tag::Plde, Tag::Leve, Tag::Wede,
    ] {
        println!("== {:?}", tag);
        for e in set.entries_of_tag(tag) {
            println!("  {:08x} {}", e.rid, e.display);
        }
    }
}

fn dims_probe() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    // Warm dims for every unit's drawn sprite + player appearance faces,
    // mirroring start_level, then report the collision-relevant sizes.
    let mut rids: Vec<ResId> = Vec::new();
    for (rid, u) in &a.units {
        rids.push(*rid);
        rids.push(u.editor_preview_sprite_face);
        for st in &u.states {
            rids.push(st.sprite_face);
        }
    }
    for w in a.weapons.values() {
        rids.push(w.player1_appearance_face);
        rids.push(w.player2_appearance_face);
    }
    let mut warmed = 0usize;
    let mut missing = 0usize;
    for rid in rids {
        if rid == pak::NONE_ID {
            continue;
        }
        let before = a.sprite_dims.len();
        a.warm_sprite_dims(rid);
        if a.sprite_dims.len() > before {
            warmed += 1;
        } else {
            missing += 1;
        }
    }
    println!("sprite_dims: warmed={warmed} missing={missing} total={}", a.sprite_dims.len());
    for rid in [pak::id("pl1c"), pak::id("buzz"), pak::id("cass"), pak::id("allp"), pak::id("plla"), pak::id("bu01")] {
        match a.sprite_dims.get(&rid) {
            Some(d) => println!("  {:08x} {}: {} frames first={:?}", rid, game::sim::rid_str(rid), d.len(), d.first()),
            None => println!("  {:08x} {}: NO DIMS", rid, game::sim::rid_str(rid)),
        }
    }
}

fn test_assets() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    println!(
        "data: {} units {} weapons {} players {} levels {} strings {} tefos",
        a.units.len(),
        a.weapons.len(),
        a.players.len(),
        a.levels.len(),
        a.strings.len(),
        a.text_formats.len()
    );
    println!(
        "lists: flli={} reli={} coli={} objids={} spriteids={}",
        a.fill_list.as_ref().map(|f| f.entries.len()).unwrap_or(0),
        a.rect_list.as_ref().map(|r| r.entries.len()).unwrap_or(0),
        a.color_list.as_ref().map(|c| c.entries.len()).unwrap_or(0),
        a.object_ids.entries.len(),
        a.sprite_ids.entries.len(),
    );

    // GIF sprite with frames.
    let sp = a.sprite(id("bgbu")).expect("bgbu sprite");
    println!(
        "bgbu: {}x{} frames={} delay0={:?}",
        sp.width,
        sp.height,
        sp.frames.len(),
        sp.frames.first().map(|f| f.delay)
    );
    // TGA background.
    let img = a.image16(id("cam1")).expect("cam1 tga");
    println!("cam1: {}x{} rgba={}", img.0, img.1, img.2.len());
    // Audio.
    for rid in [id("cohi"), id("bgbu"), id("cash")] {
        match a.sound(rid) {
            Some(s) => println!(
                "soun {:08x}: {:.2}s {}ch {}Hz {} samples",
                rid,
                s.duration_secs(),
                s.channels,
                s.sample_rate,
                s.samples.len()
            ),
            None => println!("soun {:08x}: decode failed", rid),
        }
    }
}

fn cat(tagname: &str, ridname: &str) {
    let dir = data_dir();
    let mut set = PakSet::open_dir(&dir).expect("open paks");
    let tag = Tag::from_code(tagname.as_bytes()).expect("bad tag");
    let rid = id(ridname);
    match set.read_text(tag, rid) {
        Some(t) => println!("{}", t),
        None => {
            if let Some(raw) = set.read_raw(tag, rid) {
                println!("<binary {} bytes>\n{:02x?}", raw.len(), &raw[..raw.len().min(64)]);
            } else {
                println!("not found");
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--dump-data") {
        dump_data();
        return;
    }
    if args.len() >= 4 && args[1] == "--cat" {
        cat(&args[2], &args[3]);
        return;
    }
    if args.iter().any(|a| a == "--dims") {
        dims_probe();
        return;
    }
    if args.iter().any(|a| a == "--test-assets") {
        test_assets();
        return;
    }
    if args.iter().any(|a| a == "--perm") {
        perm_probe();
        return;
    }
    if args.iter().any(|a| a == "--films") {
        film_probe();
        return;
    }
    if args.iter().any(|a| a == "--filmtest") {
        film_test();
        return;
    }
    if args.iter().any(|a| a == "--pickups") {
        pickups_probe();
        return;
    }
    if args.iter().any(|a| a == "--weapons") {
        weapon_probe();
        return;
    }
    if args.iter().any(|a| a == "--playerdefs") {
        player_probe();
        return;
    }
    if args.iter().any(|a| a == "--levels") {
        levels_probe();
        return;
    }
    if args.iter().any(|a| a == "--states") {
        let v: u32 = std::env::args().nth(2).unwrap().parse().unwrap();
        state_probe(v);
        return;
    }
    if args.iter().any(|a| a == "--coincensus") {
        coin_census();
        return;
    }
    if args.iter().any(|a| a == "--scores") {
        score_census();
        return;
    }
    if args.iter().any(|a| a == "--bonus") {
        bonus_census();
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--spawners") {
        let v: u32 = u32::from_str_radix(&args[pos + 1], 16).unwrap();
        spawner_census(v);
        return;
    }
    if args.iter().any(|a| a == "--flags") {
        flag_census();
        return;
    }
    if args.iter().any(|a| a == "--sprite") {
        let s = std::env::args().nth(2).unwrap();
        sprite_probe(&s);
        return;
    }
    if args.iter().any(|a| a == "--unit") {
        let v: u32 = std::env::args().nth(2).unwrap().parse().unwrap();
        test_unit(v);
        return;
    }
    if args.iter().any(|a| a == "--test-defs") {
        test_defs();
        return;
    }
    if args.iter().any(|a| a == "--mediacheck") {
        test_media();
        return;
    }
    if args.iter().any(|a| a == "--test-floats") {
        test_floats();
        return;
    }
    if args.iter().any(|a| a == "--test-all") {
        test_all_images_sounds();
        return;
    }
    if args.iter().any(|a| a == "--test-sprites") {
        test_sprites();
        return;
    }
    if args.iter().any(|a| a == "--rules") {
        dump_rules();
        return;
    }
    if args.iter().any(|a| a == "--strings") {
        test_strings();
        return;
    }
    if args.iter().any(|a| a == "--test-sim") {
        test_sim();
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--rawim16") {
        let rid = args.get(pos + 1).map(|s| pak::id(s)).unwrap_or_else(|| pak::id("cat1"));
        let dir = data_dir();
        let mut a = assets::GameAssets::open(&dir).expect("open");
        a.load_all_data();
        let raw = a.set.read_raw(pak::Tag::Im16, rid).expect("im16 rid");
        println!("im16 {:08x}: {} bytes", rid, raw.len());
        println!("header: {:02x?}", &raw[..raw.len().min(18)]);
        // TGA: idlen(0) cmaptype(1) imgtype(2) cmap(3..7) x(8..9) y(10..11)
        // w(12..13) h(14..15) bpp(16) desc(17)
        if raw.len() >= 18 {
            let idlen = raw[0] as usize;
            let cmap = u16::from_le_bytes([raw[5], raw[6]]);
            let ityp = raw[2];
            let w = u16::from_le_bytes([raw[12], raw[13]]);
            let h = u16::from_le_bytes([raw[14], raw[15]]);
            let bpp = raw[16];
            let desc = raw[17];
            println!("idlen={} cmaplen={} imgtype={} w={} h={} bpp={} desc={:#04x}",
                idlen, cmap, ityp, w, h, bpp, desc);
            let data_off = 18 + idlen + cmap as usize * (if raw[7] == 1 { raw[6] as usize / 8 } else { 0 });
            println!("data_off={} first64={:02x?}", data_off, &raw[data_off..(data_off + 64).min(raw.len())]);
            // u16 pixel census of first rows
            let px = &raw[data_off..];
            let mut hist: std::collections::BTreeMap<u16, usize> = Default::default();
            for chunk in px.chunks_exact(2).take(20000) {
                *hist.entry(u16::from_le_bytes([chunk[0], chunk[1]])).or_insert(0) += 1;
            }
            for (v, c) in hist.iter().take(15) { println!("  px {:#06x}: {}", v, c); }
            // full-image census of marker pixels + bounding box
            let wpx = w as usize; let hpx = h as usize;
            let mut minx = usize::MAX; let mut maxx = 0usize;
            let mut miny = usize::MAX; let mut maxy = 0usize;
            let mut count = 0usize;
            let mut rowhist: std::collections::BTreeMap<usize, usize> = Default::default();
            for y in 0..hpx {
                for x in 0..wpx {
                    let o = data_off + (y * wpx + x) * 2;
                    if o + 2 > raw.len() { break; }
                    let v = u16::from_le_bytes([raw[o], raw[o + 1]]);
                    if v == 0x1f {
                        count += 1;
                        minx = minx.min(x); maxx = maxx.max(x);
                        miny = miny.min(y); maxy = maxy.max(y);
                        *rowhist.entry(y).or_insert(0) += 1;
                    }
                }
            }
            println!("  0x1f count={} bbox x[{},{}] y[{},{}] rows={}",
                count, minx, maxx, miny, maxy, rowhist.len());
            for (y, c) in rowhist.iter().take(10) { println!("    row {}: {}", y, c); }
            // Orientation check: decode the level bg via image16 and
            // profile blue-dominant (water) pixels per 40px band, then
            // compare with the 0x1f band under both row orders.
            let bg_rid = a.levels.values()
                .find(|l| l.media_mask == rid)
                .map(|l| l.background_image);
            println!("  level bg rid: {:?}", bg_rid.map(|r| format!("{:08x}", r)));
            if let Some(brid) = bg_rid {
                if let Some(bg) = a.image16(brid).map(|i| (i.0, i.1, i.2.clone())) {
                    let (bw, bh, rgba) = bg;
                    println!("  bg {}x{}", bw, bh);
                    for band in 0..(bh as usize / 40) {
                        let mut blue = 0usize; let mut tot = 0usize;
                        for y in band * 40..(band + 1) * 40 {
                            for x in 0..bw as usize {
                                let o = (y * bw as usize + x) * 4;
                                let (r, g, b) = (rgba[o] as i32, rgba[o + 1] as i32, rgba[o + 2] as i32);
                                if b > r + 30 && b > g + 30 { blue += 1; }
                                tot += 1;
                            }
                        }
                        if blue * 20 > tot { println!("    bgband {} (y {}..{}): {}/{} blue", band, band * 40, band * 40 + 39, blue, tot); }
                    }
                }
            }
        }
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--objdump") {
        let rid = args.get(pos + 1).map(|s| pak::id(s)).unwrap_or_else(|| pak::id("le07"));
        let dir = data_dir();
        let mut a = assets::GameAssets::open(&dir).expect("open");
        a.load_all_data();
        let lv = a.levels.get(&rid).expect("level");
        let mut ys: Vec<i64> = lv.objects.iter().map(|o| o.y).collect();
        ys.sort();
        println!("level {:?}: {} objects, bg={:?}", rid, ys.len(), lv.background);
        println!("y min={} max={} first10={:?} last10={:?}",
            ys.first().copied().unwrap_or(-1), ys.last().copied().unwrap_or(-1),
            &ys[..ys.len().min(10)], &ys[ys.len().saturating_sub(10)..]);
        // histogram per 64px band
        let mut bands: std::collections::BTreeMap<i64, usize> = Default::default();
        for &y in &ys { *bands.entry(y / 64).or_insert(0) += 1; }
        for (b, c) in bands.iter().take(20) { println!("  band {} (y {}..{}): {}", b, b*64, b*64+63, c); }
        // scatter-mode census: both axes ranged -> polar scatter in the
        // original (sub_41c540); single-axis -> independent roll.
        let mut polar = 0; let mut xonly = 0; let mut yonly = 0; let mut fixed = 0;
        for (rid, u) in &a.units {
            let xr = u.x_offset_min != u.x_offset_max;
            let yr = u.y_offset_min != u.y_offset_max;
            match (xr, yr) {
                (true, true) => { polar += 1; println!("  POLAR {:?} x[{},{}] y[{},{}]", rid, u.x_offset_min, u.x_offset_max, u.y_offset_min, u.y_offset_max); }
                (true, false) => xonly += 1,
                (false, true) => yonly += 1,
                (false, false) => fixed += 1,
            }
        }
        println!("scatter census: polar={} xonly={} yonly={} fixed={}", polar, xonly, yonly, fixed);
        return;
    }
    // Default: launch the game.
    let players = if args.iter().any(|a| a == "--2p" || a == "--players") {
        args.iter()
            .position(|a| a == "--players")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(2)
    } else {
        1
    };
    let dir = data_dir();
    let mut assets = assets::GameAssets::open(&dir).expect("open retail data");
    assets.load_all_data();
    let level = args
        .iter()
        .position(|a| a == "--level")
        .and_then(|i| args.get(i + 1))
        .map(|s| pak::id(s))
        .unwrap_or_else(|| pak::id("le01"));
    game::render::run(assets, level, players);
}

fn test_sprites() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    // Every lowercase im08 rid should have an uppercase alpha partner.
    let mut rids: Vec<pak::ResId> = a
        .set
        .entries_of_tag(Tag::Im08)
        .iter()
        .map(|e| e.rid)
        .collect();
    rids.sort();
    rids.dedup();
    let mut ok = 0;
    let mut fail = 0;
    let mut total_frames = 0usize;
    let mut hist: std::collections::BTreeMap<usize, usize> = Default::default();
    for rid in rids {
        // only decode colour (lowercase) plates; skip pure-uppercase alpha ids
        let b = rid.to_le_bytes();
        if b.iter().all(|c| c.is_ascii_uppercase()) {
            continue;
        }
        match a.sprite(rid) {
            Some(s) => {
                ok += 1;
                total_frames += s.frames.len();
                *hist.entry(s.frames.len()).or_default() += 1;
                if s.frames.is_empty() {
                    println!("  EMPTY {:08x}", rid);
                }
            }
            None => {
                fail += 1;
                println!("  FAIL {:08x}", rid);
            }
        }
    }
    println!("sprites: {} ok {} fail, {} total frames", ok, fail, total_frames);
    println!("frame-count histogram: {:?}", hist);
}

fn test_all_images_sounds() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    // im16 TGA images
    let mut rids: Vec<pak::ResId> = a.set.entries_of_tag(Tag::Im16).iter().map(|e| e.rid).collect();
    rids.sort();
    rids.dedup();
    let (mut ok, mut fail) = (0, 0);
    for rid in rids {
        match a.image16(rid) {
            Some((w, h, px)) => { ok += 1; let _ = (w, h, px.len()); }
            None => { fail += 1; println!("  im16 FAIL {:08x}", rid); }
        }
    }
    println!("im16: {} ok {} fail", ok, fail);
    // sounds across all paks
    let mut rids: Vec<pak::ResId> = a.set.entries_of_tag(Tag::Soun).iter().map(|e| e.rid).collect();
    rids.sort();
    rids.dedup();
    let (mut ok, mut fail) = (0, 0);
    for rid in rids {
        match a.sound(rid) {
            Some(s) => { ok += 1; let _ = s; }
            None => { fail += 1; println!("  soun FAIL {:08x}", rid); }
        }
    }
    println!("soun: {} ok {} fail", ok, fail);
}

fn test_floats() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    if let Some(fl) = &a.fill_list {
        for i in 0x00usize..0x20 {
            println!("flli[0x{:02x}] = {}", i, fl.get(i));
        }
        for i in 0x30usize..0x45 {
            println!("flli[0x{:02x}] = {}", i, fl.get(i));
        }
        for i in 0x88usize..0x98 {
            println!("flli[0x{:02x}] = {}", i, fl.get(i));
        }
        for i in 0x98usize..0xb0 {
            println!("flli[0x{:02x}] = {}", i, fl.get(i));
        }
        for i in 0xb0usize..0xd0 {
            println!("flli[0x{:02x}] = {}", i, fl.get(i));
        }
        for i in 0xd0usize..0xe0 {
            println!("flli[0x{:02x}] = {}", i, fl.get(i));
        }
    }
    println!("object_ids: {} entries", a.object_ids.entries.len());
    for (i, (n, r)) in a.object_ids.entries.iter().enumerate().take(0x28) {
        println!("  obj[0x{:02x}] name={:?} id={:08x}", i, n, r);
    }
}

fn test_defs() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    for (rid,p) in &a.players {
        println!("PLAYER {:08x} {:?}", rid, p);
    }
    for (rid,w) in &a.weapons {
        println!("WEAPON {:08x} {:?}", rid, w);
    }
    // group-spawn flag census
    let mut grp=0; let mut nospawn=0; let mut delown=0; let mut onlyact=0;
    let mut grp_units=Vec::new();
    for (rid,u) in &a.units {
        if u.num_in_group_max > 1 || u.num_in_group_min > 1 { grp+=1; grp_units.push((*rid,u.name.clone())); }
        if u.do_not_spawn_if_type_exists { nospawn+=1; }
        if u.delete_existing_owned_by_player { delown+=1; }
        if u.spawn_only_when_players_active { onlyact+=1; }
    }
    println!("UNITS total={} groups={} nospawn={} delown={} onlyactive={}", a.units.len(), grp, nospawn, delown, onlyact);
    let mut appother=Vec::new(); let mut grpdetail=Vec::new();
    for (rid,u) in &a.units {
        if u.appears_percent != 100 { appother.push((*rid,u.name.clone(),u.appears_percent)); }
        if u.num_in_group_min != 1 || u.num_in_group_max != 1 { grpdetail.push((*rid,u.name.clone(),(u.num_in_group_min,u.num_in_group_max))); }
    }
    println!("UNITS appearsOther={:?}", appother);
    println!("UNITS grpDetail={:?}", grpdetail);
    let mut nlock=0; let mut nlink=0; let mut norb=0; let mut ncyc=0; let mut nhold=0;
    for u in a.units.values() { for st in &u.states {
        if st.lock_to_owner_loc {nlock+=1;} if st.link_to_owner_loc {nlink+=1;} if st.orbit_owner {norb+=1;}
        if st.cyclic_motion {ncyc+=1;} if st.hold_position_to_target {nhold+=1;}
    }}
    println!("STATES lock={} link={} orbit={} cyclic={} hold={}", nlock,nlink,norb,ncyc,nhold);
    let mut ncoll=0; let mut ncwp=0; let mut ntarg=0; let mut nnodepl=0; let mut nrev=0; let mut nshieldst=0; let mut nusevis=0; let mut nusescale=0; let mut ndestowner=0; let mut nvisrefl=0; let mut nparentdir=0; let mut ncolspawn=0;
    for u in a.units.values() { for st in &u.states {
        if st.collides {ncoll+=1;} if st.collides_with_players {ncwp+=1;} if st.is_targetable {ntarg+=1;}
        if st.shields_do_not_deplete_on_collision {nnodepl+=1;} if st.reverse_direction_on_reaction {nrev+=1;}
        if st.use_on_shield_depletion {nshieldst+=1;} if st.use_owners_visibility {nusevis+=1;}
        if st.use_owners_scale {nusescale+=1;} if st.destroy_owner_on_destruction {ndestowner+=1;}
        if st.visually_reflect_owner_hits {nvisrefl+=1;} if st.use_parent_direction {nparentdir+=1;}
        if st.collision_spawn_id != pak::NONE_ID {ncolspawn+=1;}
    }}
    println!("STATES collides={} collPlayers={} targetable={} noDeplete={} reverse={} shieldState={} useVis={} useScale={} destOwner={} visRefl={} parentDir={} colSpawn={}", ncoll,ncwp,ntarg,nnodepl,nrev,nshieldst,nusevis,nusescale,ndestowner,nvisrefl,nparentdir,ncolspawn);
    // Particle config census.
    let mut dp=0; let mut hp=0; let mut sp=0; let mut spr=0;
    for u in a.units.values() {
        if u.destruct_particle != pak::NONE_ID {dp+=1;}
        if u.hit_particles != pak::NONE_ID {hp+=1;}
        for st in &u.states {
            if st.particles_id != pak::NONE_ID {sp+=1; if st.particles_repeat {spr+=1;}}
        }
    }
    println!("PARTICLES destruct={} hit={} state={} stateRepeat={}", dp,hp,sp,spr);
    // Ground-accuracy target census.
    let acc: Vec<String> = a.units.values()
        .filter(|u| u.include_in_ground_accuracy)
        .map(|u| format!("{}", u.name)).collect();
    println!("ACCURACY targets={}", acc.len());
    for n in acc.iter().take(40) { println!("   {}", n); }
    // Debris/ground-obstacle flag census.
    let gobs: Vec<String> = a.units.values()
        .filter(|u| u.collides_with_ground_obstacles)
        .map(|u| format!("{}", u.name)).collect();
    let mkobs: Vec<String> = a.units.values()
        .filter(|u| u.destruct_create_obstacle)
        .map(|u| format!("{}", u.name)).collect();
    println!("DEBRIS collideWithGroundObs={} destructCreateObstacle={}", gobs.len(), mkobs.len());
    for n in gobs.iter().take(30) { println!("   cwg {}", n); }
    for n in mkobs.iter().take(30) { println!("   dco {}", n); }
    // Shield level-scaling census: base/inc/max per unit.
    let mut sh_pos=0; let mut sh_neg=0; let mut sh_zero=0;
    for (rid,u) in &a.units {
        if u.shields_base != 0.0 || u.shields_level_increment != 0.0 {
            if u.shields_level_increment > 0.0 {sh_pos+=1} else if u.shields_level_increment < 0.0 {sh_neg+=1} else {sh_zero+=1}
            if sh_pos+sh_neg+sh_zero <= 15 {
                println!("SHIELD {:08x} {} base={} inc={} max={}", rid, u.name, u.shields_base, u.shields_level_increment, u.shields_max);
            }
        }
    }
    println!("SHIELDS inc+={} inc-={} inc0={}", sh_pos,sh_neg,sh_zero);
    // on_counter census: how many states chain after N entries.
    let mut oc=0; let mut oc1=0;
    for u in a.units.values() { for st in &u.states {
        if st.on_counter > 0 { oc+=1; if st.on_counter == 1 {oc1+=1;}
            if oc <= 12 { println!("ONCNT {}.{} n={} -> {}", u.name, st.name, st.on_counter, st.on_counter_change_to); }
        }
    }}
    println!("ONCOUNTER states={} ofWhich1={}", oc, oc1);
    // Notice census.
    let mut dn=0; let mut en=0; let mut ed=0;
    for u in a.units.values() {
        if !u.destruct_notice.is_empty() && u.destruct_notice != "none" {dn+=1;}
        if !u.entry_notice.is_empty() && u.entry_notice != "none" {en+=1; if u.entry_notice_delay>0 {ed+=1;}}
    }
    println!("NOTICES destruct={} entry={} entryDelay={}", dn,en,ed);
    // Player-damage-source census: which units can hurt the player on contact.
    // Retail requires state.passHitsToOwner + a live owner (G_EntityGroup
    // 0x418d44); damage comes from fileData->damage (#damage_FLOAT).
    let mut dmg_units: Vec<String> = Vec::new();
    let mut pho_dmg = 0usize;
    for (rid, u) in &a.units {
        if u.damage > 0.0 && !u.player_projectile && !u.terrain_effect && !u.harmless_to_players {
            let pho = u.states.iter().any(|s| s.pass_hits_to_owner);
            if pho { pho_dmg += 1; }
            dmg_units.push(format!("{}:{:08x}{}", u.name, rid, if pho {"*"} else {""}));
        }
    }
    dmg_units.sort();
    println!("DMG-SOURCES units={} withPassHits={} ( *=passHitsToOwner)", dmg_units.len(), pho_dmg);
    for n in dmg_units.iter().take(60) { println!("   {}", n); }
    // Random-bonus drop census: which units release a bonus pod on death,
    // and what object_ids[0x19..0x22] resolve to.
    let mut rb: Vec<String> = Vec::new();
    for (rid, u) in &a.units {
        if u.destruct_release_random_bonus {
            rb.push(format!("{}:{:08x}", u.name, rid));
        }
    }
    rb.sort();
    println!("RANDOMBONUS units={} {:?}", rb.len(), rb);
    for slot in 0x19usize..=0x22 {
        let r = a.object_ids.entries.get(slot).map(|e| e.1);
        println!("   objid[{:02x}] = {:08x}", slot, r.unwrap_or(0));
    }
    // Heading-tolerance / owner-heading census: the original only rolls
    // initialHeadingTolerance for useOwnerHeading units without an owner.
    let (mut ntol, mut nown, mut ntol_own) = (0, 0, 0);
    for (rid,u) in &a.units {
        if u.initial_heading_tolerance != 0 {
            ntol += 1;
            if u.use_owner_heading { ntol_own += 1; }
            if ntol <= 15 {
                println!("HTOL {:08x} {} tol={} ownHdg={} edtr={} hunts={}", rid, u.name, u.initial_heading_tolerance, u.use_owner_heading, u.initial_heading_set_in_editor, u.initially_hunts_closest_player);
            }
        }
        if u.use_owner_heading { nown += 1; }
    }
    println!("HTOL tolerance={} useOwnerHeading={} tol+own={}", ntol, nown, ntol_own);
    for u in a.units.values() {
        if !u.entry_notice.is_empty() && u.entry_notice != "none" {
            println!("  entryNotice '{}' delay={} unit?", &u.entry_notice[..u.entry_notice.len().min(60)], u.entry_notice_delay);
        }
    }
    // State sound-loop census.
    let mut sl=0; let mut slm=0; let mut sr=0; let mut so=0; let mut snd=0;
    for u in a.units.values() { for st in &u.states {
        if st.entry_sound.id != pak::NONE_ID {snd+=1;}
        if st.sound_loop {sl+=1;} if st.sound_max_num_to_play>0 {slm+=1;}
        if st.sound_repeat_on_state_change {sr+=1;} if st.sound_allow_only_one_instance {so+=1;}
    }}
    println!("STATE-SND entry={} loop={} maxNum={} repeatChg={} oneInst={}", snd,sl,slm,sr,so);
    let mut mb=0; let mut mba=0;
    for u in a.units.values() { for st in &u.states {
        if st.motion_blur_required {mb+=1; if st.motion_blur_allow_glow {mba+=1;}}
    }}
    println!("MOTIONBLUR required={} glow={}", mb,mba);
    // Weapon field census.
    let mut wammo=0; let mut wauto=0; let mut wload=0; let mut wch=0;
    for w in a.weapons.values() {
        if w.num_ammo_in_pack>0 {wammo+=1;} if w.auto_repeat {wauto+=1;}
        if w.delay_between_load_launches>0 {wload+=1;} if w.crosshair_face!=pak::NONE_ID {wch+=1;}
    }
    println!("WEAPONS ammo={} autoRep={} loadDelay={} crosshair={}", wammo,wauto,wload,wch);
    // pickup units: do their states set collides/collidesWithPlayers?
    let mut pu=0; let mut pcoll=0; let mut pcwp=0; let mut pnames=Vec::new();
    for (rid,u) in &a.units {
        if u.pickup_type == pak::NONE_ID { continue; }
        pu+=1;
        let any_coll = u.states.iter().any(|st| st.collides);
        let any_cwp = u.states.iter().any(|st| st.collides_with_players);
        if any_coll {pcoll+=1;} if any_cwp {pcwp+=1;}
        if pnames.len()<8 { pnames.push(format!("{:08x} {} coll={} cwp={} pt={:08x}", rid,u.name,any_coll,any_cwp,u.pickup_type)); }
    }
    println!("PICKUPS units={} anyCollides={} anyCollPlayers={}", pu,pcoll,pcwp);
    for n in pnames { println!("  {}", n); }
    // damage census: which units hurt players on contact?
    let mut hurts: Vec<String> = Vec::new();
    for (rid,u) in &a.units {
        if u.damage > 0.0 && !u.harmless_to_players {
            hurts.push(format!("{:08x} {} dmg={} proj={} terr={}", rid,u.name,u.damage,u.player_projectile,u.terrain_effect));
        }
    }
    println!("HARMFUL units={}", hurts.len());
    for n in hurts.iter().take(25) { println!("  {}", n); }
    for (r,n) in grp_units.iter().take(20) { println!("  grp {:08x} {}", r, n); }
    // media-gate census: CanSpawnOnMedia applies when isGroundBased &&
    // !doDeathSpawnOnAnyMedia && destructSpawn != none.
    let mut mg = 0; let mut mg_any = 0; let mut mis_hist: std::collections::BTreeMap<ResId, usize> = Default::default();
    for (rid,u) in &a.units {
        if u.is_ground_based {
            mg += 1;
            if u.do_death_spawn_on_any_media { mg_any += 1; }
            if !u.do_death_spawn_on_any_media && u.destruct_spawn != pak::NONE_ID {
                *mis_hist.entry(u.media_impact_size).or_insert(0) += 1;
                println!("  MEDIA {:08x} {} mis={:08x} dsp={:08x}", rid, u.name, u.media_impact_size, u.destruct_spawn);
            }
        }
    }
    println!("MEDIA ground={} anyMedia={} gated={}", mg, mg_any, mis_hist.values().sum::<usize>());
    for (k,v) in &mis_hist { println!("  mis {:08x}: {}", k, v); }
    // ALL units' mediaImpactSize census (reap path reaches every dead entity)
    let mut allmis: std::collections::BTreeMap<ResId, usize> = Default::default();
    for (rid,u) in &a.units {
        *allmis.entry(u.media_impact_size).or_insert(0) += 1;
        if u.media_impact_size != pak::NONE_ID {
            println!("  MISUNIT {:08x} {} mis={:08x} ground={} any={} dsp={:08x}",
                rid, u.name, u.media_impact_size, u.is_ground_based, u.do_death_spawn_on_any_media, u.destruct_spawn);
        }
    }
    for (k,v) in &allmis { println!("  allmis {:08x}: {}", k, v); }
    // deletionSpawn / destructSpawn census — CanSpawnOnMedia gates both.
    let mut ndel = 0; let mut ndsp_ground = 0;
    for (rid,u) in &a.units {
        if u.deletion_spawn != pak::NONE_ID { ndel += 1; }
        if u.destruct_spawn != pak::NONE_ID && u.is_ground_based { ndsp_ground += 1; }
        if u.deletion_spawn != pak::NONE_ID && u.media_impact_size != pak::NONE_ID {
            println!("  DEL+MEDIA {:08x} {} del={:08x} mis={:08x}", rid, u.name, u.deletion_spawn, u.media_impact_size);
        }
    }
    println!("deletionSpawn units={} groundDestructSpawn={}", ndel, ndsp_ground);
    let ncoin = a.units.values().filter(|u| u.destruct_coin != pak::NONE_ID).count();
    let ngoins = a.units.values().filter(|u| u.destruct_coin_on_group_kill != pak::NONE_ID).count();
    println!("destructCoin units={} groupKillCoin={}", ncoin, ngoins);
    // Owner-cascade census: destructDestroy/DeleteChildren on defs plus
    // the per-state canBe*OnOwner* flags that gate which children join.
    let (mut nddc, mut nddel, mut nst_destroy, mut nst_delete) = (0, 0, 0, 0);
    for u in a.units.values() {
        if u.destruct_destroy_children { nddc += 1; }
        if u.destruct_delete_children { nddel += 1; }
        for s in &u.states {
            if s.destroy_on_owner_destruction { nst_destroy += 1; }
            if s.delete_on_owner_deletion { nst_delete += 1; }
        }
    }
    println!("cascade: destructDestroyChildren={} destructDeleteChildren={} stateCanDestroy={} stateCanDelete={}",
        nddc, nddel, nst_destroy, nst_delete);
    for (rid,u) in a.units.iter().filter(|(_,u)| u.destruct_destroy_children || u.destruct_delete_children).take(10) {
        println!("  CASC {:08x} {} ddc={} ddel={}", rid, u.name, u.destruct_destroy_children, u.destruct_delete_children);
    }
    for (rid,u) in a.units.iter().filter(|(_,u)| u.destruct_coin != pak::NONE_ID).take(4) {
        println!("  COIN {:08x} {} coin={:08x} n={} gid-kill={:08x}", rid, u.name, u.destruct_coin, u.destruct_num_coins, u.destruct_coin_on_group_kill);
    }
    // Scatter census: polar needs both offset axes ranged; randomiseInitialLoc
    // picks circular (angle+radius draws) vs ellipse (angle draw only).
    let (mut nper, mut nfix, mut pr, mut pe) = (0, 0, 0, 0);
    for u in a.units.values() {
        let xr = u.x_offset_min != u.x_offset_max;
        let yr = u.y_offset_min != u.y_offset_max;
        if xr && yr {
            if u.randomise_initial_loc { pr += 1; } else { pe += 1; }
        } else if xr || yr {
            nper += 1;
        } else {
            nfix += 1;
        }
    }
    println!("scatter: polarRand={} polarEllipse={} perAxis={} fixed={}", pr, pe, nper, nfix);
    for (rid,u) in a.units.iter().filter(|(_,u)| u.x_offset_min != u.x_offset_max && u.y_offset_min != u.y_offset_max && !u.randomise_initial_loc).take(10) {
        println!("  ELLIP {:08x} {} x=[{}..{}] y=[{}..{}]", rid, u.name, u.x_offset_min, u.x_offset_max, u.y_offset_min, u.y_offset_max);
    }
    // le01 objects sample
    if let Some(l) = a.levels.get(&id("le01")) {
        println!("LEVEL le01 bg={:?} img={:08x} music={:08x} objs={}", l.background, l.background_image, l.music, l.objects.len());
        for o in l.objects.iter().take(15) {
            println!("  obj unit={:08x} layer={:08x} x={} y={} hdg={} stat={} terr={}", o.unit_id,o.layer_id,o.x,o.y,o.heading_degrees,o.stationary,o.terrain_effects);
        }
    }
}

fn test_strings() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut keys: Vec<ResId> = a.strings.keys().copied().collect();
    keys.sort();
    for k in keys {
        let s = &a.strings[&k];
        println!("=== stli {:08x} ({} strings)", k, s.strings.len());
        for (i, line) in s.strings.iter().enumerate() {
            println!("  [{}] {}", i, line);
        }
    }
    for (rid, l) in &a.levels {
        println!(
            "LEVEL {:08x} name={:?} id={:?} briefing={:08x} music={:08x} objs={}",
            rid, l.name, l.identifier, l.briefing, l.music, l.objects.len()
        );
        if l.briefing != pak::NONE_ID {
            if let Some(s) = a.strings.get(&l.briefing) {
                for line in s.strings.iter().take(10) {
                    println!("    brief: {}", line);
                }
            }
        }
    }
}

fn player_probe() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut v: Vec<_> = a.players.iter().collect();
    v.sort_by_key(|(r, _)| **r);
    for (rid, p) in v {
        println!(
            "{:08x} lives_start={} lives_max={} shield_start={} defbonus={:08x} hitobj={:08x} warnobj={:08x} counterobj={:08x}",
            rid, p.life_num_initial, p.life_max_num, p.default_shield_percentage,
            p.active_defence_bonus_object, p.active_spawn_on_hit,
            p.active_shield_warning_object, p.active_money_counter_spawn
        );
    }
}

fn levels_probe() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut v: Vec<_> = a.levels.iter().collect();
    v.sort_by_key(|(r, _)| **r);
    for (rid, l) in v {
        println!(
            "{:08x} name={:?} ident={:?} objs={} music={:08x}",
            rid, l.name, l.identifier, l.objects.len(), l.music
        );
    }
}

fn pickups_probe() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut v: Vec<_> = a
        .units
        .iter()
        .filter(|(_, u)| u.pickup_type != pak::NONE_ID)
        .map(|(&rid, u)| (rid, u.name.clone(), u.pickup_type, u.pickup_value))
        .collect();
    v.sort();
    for (rid, name, pt, pv) in v {
        println!("{:08x} {:30} pickup={:08x} val={}", rid, name, pt, pv);
    }
}


fn score_census() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut v: Vec<_> = a.units.iter().collect();
    v.sort_by_key(|(_, u)| u.score);
    for (rid, u) in v.iter().take(15) {
        println!("score={:>8} {:08x} {}", u.score, rid, u.name);
    }
    println!("--- top ---");
    for (rid, u) in v.iter().rev().take(5) {
        println!("score={:>8} {:08x} {}", u.score, rid, u.name);
    }
}

fn flag_census() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut counts: std::collections::HashMap<&'static str, usize> = Default::default();
    let mut names: std::collections::HashMap<&'static str, Vec<String>> = Default::default();
    let mut notice_users = 0usize;
    for (rid, u) in &a.units {
        if !u.entry_notice.is_empty() || !u.destruct_notice.is_empty() {
            notice_users += 1;
            if notice_users <= 10 {
                println!("notice user {:08x} {} entry='{:.30}' destruct='{:.30}'", rid, u.name, u.entry_notice, u.destruct_notice);
            }
        }
        for st in &u.states {
            let checks: &[(&'static str, bool)] = &[
                ("is_targetable", st.is_targetable),
                ("destroy_owner_on_destruction", st.destroy_owner_on_destruction),
                ("use_owners_visibility", st.use_owners_visibility),
                ("use_owners_scale", st.use_owners_scale),
                ("visually_reflect_owner_hits", st.visually_reflect_owner_hits),
                ("pickup_no_appearance_change", st.pickup_no_appearance_change),
                ("reverse_direction_on_reaction", st.reverse_direction_on_reaction),
                ("draw_to_terrain", st.draw_to_terrain),
                ("use_on_shield_depletion", st.use_on_shield_depletion),
                ("do_not_glow_on_collision", st.do_not_glow_on_collision),
                ("do_colorise", st.do_colorise),
                ("delete_on_no_active_players", st.delete_on_no_active_players),
                ("destruct_on_no_active_players", st.destruct_on_no_active_players),
                ("on_counter", st.on_counter != 0),
                ("flee_id", st.flee_id != crate::pak::NONE_ID),
                ("collision_spawn", st.collision_spawn_id != crate::pak::NONE_ID),
                ("collision_repeat", st.collision_repeat_spawns),
                ("rotate_to_target", st.rotate_to_target),
                ("pass_hits_to_owner", st.pass_hits_to_owner),
                ("pause_vertical_scrolling", st.pause_vertical_scrolling),
                ("rule_unit", st.rules.iter().any(|r| r.unit_id != crate::pak::NONE_ID)),
            ];
            for (n, v) in checks {
                if *v {
                    *counts.entry(n).or_default() += 1;
                    names.entry(n).or_default().push(format!("{}:{}", u.name, rid));
                }
            }
            for ss in &st.spawn_sets {
                for (n, v) in [
                    ("spawn_if_fleeing", ss.spawn_if_fleeing),
                    ("terrain_effects_option", ss.terrain_effects_option),
                ] {
                    if v {
                        *counts.entry(n).or_default() += 1;
                        names.entry(n).or_default().push(format!("{}:{}", u.name, rid));
                    }
                }
            }
        }
        for (n, v) in [
            ("do_burst", u.do_burst),
            ("do_implode", u.do_implode),
            ("destruct_draw_to_terrain", u.destruct_draw_to_terrain),
            ("init_hunts_closest", u.initially_hunts_closest_player),
            ("use_owner_heading", u.use_owner_heading),
            ("heading_set_in_editor", u.initial_heading_set_in_editor),
        ] {
            if v {
                *counts.entry(n).or_default() += 1;
                names.entry(n).or_default().push(format!("{}", u.name));
            }
        }
    }
    println!("notice-string users: {}", notice_users);
    // Full passHitsToOwner unit census (player-contact damage gate).
    let mut pho: Vec<String> = Vec::new();
    for (rid, u) in &a.units {
        if u.states.iter().any(|s| s.pass_hits_to_owner) {
            pho.push(format!("{}:{:08x}", u.name, rid));
        }
    }
    pho.sort(); pho.dedup();
    println!("passHitsToOwner units ({}): {:?}", pho.len(), pho);
    // Distinct flee_id values in use (for the Priv_Flee arm census).
    let mut flee_vals: Vec<String> = Vec::new();
    for u in a.units.values() {
        for st in &u.states {
            if st.flee_id != crate::pak::NONE_ID {
                let s = game::sim::rid_str(st.flee_id);
                if !flee_vals.contains(&s) {
                    flee_vals.push(s);
                }
            }
        }
    }
    flee_vals.sort();
    println!("flee_ids: {:?}", flee_vals);
    let mut ks: Vec<_> = counts.keys().collect();
    ks.sort();
    for k in ks {
        println!("{}: {}  {:?}", k, counts[k], &names[k][..names[k].len().min(6)]);
    }
}

fn sprite_probe(name: &str) {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    let rid = pak::id(name);
    if let Some(s) = a.sprite(rid) {
        println!(
            "sprite {:08x}: {} frames, {}x{}",
            rid,
            s.frames.len(),
            s.width,
            s.height
        );
        for (i, f) in s.frames.iter().enumerate() {
            println!("  f{} {}x{}", i, f.width, f.height);
        }
    } else {
        println!("sprite {:08x} not found", rid);
    }
}

fn test_unit(ridv: u32) {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    if let Some(u) = a.units.get(&ridv) {
        println!("UNIT {:08x} {}", ridv, u.name);
        println!("  init_speed {}..{} heading {} tol {}", u.initial_speed_min, u.initial_speed_max, u.initial_heading, u.initial_heading_tolerance);
        println!("  shields {}/{}+{} dmg {} score {} pickup={:08x} destruct_spawn={:08x} deletion_spawn={:08x}", u.shields_base,u.shields_max,u.shields_level_increment,u.damage,u.score,u.pickup_type,u.destruct_spawn,u.deletion_spawn);
        println!("  numInGroup {}..{} groupDelay {}..{} appears%={} onlyWhenActive={} noDup={}", u.num_in_group_min,u.num_in_group_max,u.group_delay_min,u.group_delay_max,u.appears_percent,u.spawn_only_when_players_active,u.do_not_spawn_if_type_exists);
        for (i,st) in u.states.iter().enumerate() {
            println!("  state[{}] '{}' spr={:08x} dirs={} fpd={} framedly={} maxspd={} delta={} ontimer={}-{}->'{}' delNoPl={} destrNoPl={} destrScroll={} rules={:?}", i, st.name, st.sprite_face, st.num_directions, st.frames_per_direction, st.frame_delay, st.max_speed, st.delta, st.on_timer_min, st.on_timer_max, st.on_timer_change_to, st.delete_on_no_active_players, st.destruct_on_no_active_players, st.destruct_if_scrolling_not_paused, st.rules.iter().map(|r|(r.name.clone(),r.unit_id,r.range,r.condition.clone(),r.action.clone())).collect::<Vec<_>>());
            for ss in &st.spawn_sets { println!("     spawnset '{}' id={:08x} x={} y={} rate={}-{} volley={}-{} delay={}-{} repeat={}", ss.name, ss.spawn_id, ss.x_offset, ss.y_offset, ss.rate_min, ss.rate_max, ss.volley_min, ss.volley_max, ss.delay_between_min, ss.delay_between_max, ss.repeat_spawns); }
        }
    }
}

/// Media-mask end-to-end check: spawn each media-impact ground unit on and
/// off the le01 water band, destroy it, and verify the perm-object splash
/// spawn (object_ids 6..=9) appears only on media.
fn test_media() {
    use game::sim::{PlayerInput, SpawnReq};
    use glam::Vec2;
    let dir = data_dir();
    let mut assets = assets::GameAssets::open(&dir).expect("open");
    assets.load_all_data();
    let level_id = pak::id("le01");
    let mut sim = game::render::start_level(&mut assets, level_id, 1, 1).expect("level");
    let m = sim.media_mask.as_ref().expect("media mask").clone();
    let tile = sim.media_tile_size.max(1) as u32;
    let level_h = (sim.level.background[3] - sim.level.background[1]).max(1) as f32;
    println!(
        "mask {}x{} tile={} scroll={} level_h={}",
        m.w, m.h, tile, sim.scroll, level_h
    );
    // Find a media tile (0x1f) and a dry tile, preferring mid-screen x.
    // tile (tx,ty) -> loc=(tx*t+2, ty*t+2) -> pos=(loc_x-32, level_h-loc_y).
    let mut media_tile = None;
    let mut dry_tile = None;
    'scan: for ty in 0..m.h {
        for tx in 0..m.w {
            let v = m.at(tx, ty);
            if v == 0x1f && media_tile.is_none() && tx > m.w / 4 && tx < m.w * 3 / 4 {
                media_tile = Some((tx, ty));
            }
            if v != 0x1f && dry_tile.is_none() && tx > m.w / 4 && tx < m.w * 3 / 4 {
                dry_tile = Some((tx, ty));
            }
            if media_tile.is_some() && dry_tile.is_some() {
                break 'scan;
            }
        }
    }
    let tile_to_pos = |(tx, ty): (u32, u32)| -> (Vec2, f32) {
        let loc_x = (tx * tile + tile / 2) as f32;
        let loc_y = (ty * tile + tile / 2) as f32;
        let pos = Vec2::new(loc_x - 32.0, level_h - loc_y);
        // Park scroll so pos.y sits mid-view.
        let scroll = (pos.y - 240.0).clamp(0.0, sim.max_scroll());
        (pos, scroll)
    };
    let (media_pos, media_scroll) = media_tile.map(tile_to_pos).unwrap_or((Vec2::ZERO, 0.0));
    let (dry_pos, dry_scroll) = dry_tile.map(tile_to_pos).unwrap_or((Vec2::ZERO, 0.0));
    println!(
        "media_tile={media_tile:?} pos={media_pos:?} scroll={media_scroll} | dry_tile={dry_tile:?} pos={dry_pos:?} scroll={dry_scroll}"
    );
    let mut media_rids = std::collections::HashSet::new();
    for s in 6..=9usize {
        if let Some((name, rid)) = assets.object_ids.entries.get(s) {
            println!(
                "  obj[{}] {:08x} {} unit_def={}",
                s,
                rid,
                name,
                assets.units.contains_key(rid)
            );
            media_rids.insert(*rid);
        }
    }
    let count_media = |sim: &game::sim::Sim| -> Vec<ResId> {
        sim.entities
            .iter()
            .filter(|e| !e.dead && media_rids.contains(&e.unit))
            .map(|e| e.unit)
            .collect()
    };
    for unit in [0x72746174u32, 0x72636f62, pak::id("plbo")] {
        let uname = assets
            .units
            .get(&unit)
            .map(|u| u.name.clone())
            .unwrap_or_else(|| format!("{:08x}", unit));
        if let Some(d) = assets.units.get(&unit) {
            println!(
                "  {uname}: states={} grp={}-{} appears={}% ponly={} notype={} ground={}",
                d.states.len(),
                d.num_in_group_min,
                d.num_in_group_max,
                d.appears_percent,
                d.spawn_only_when_players_active,
                d.do_not_spawn_if_type_exists,
                d.is_ground_based
            );
        }
        for (label, pos, scroll) in [
            ("media", media_pos, media_scroll),
            ("dry", dry_pos, dry_scroll),
        ] {
            let before = count_media(&sim).len();
            // Entities outside the visible window [scroll, scroll+480]
            // are culled on the first step — park the camera so the test
            // position is on-screen. The media lookup is scroll-invariant.
            sim.scroll = scroll;
            // roll_spawn_count can legitimately return 0 — retry until one
            // materialises (each attempt re-rolls).
            let mut idx = None;
            'spawn: for _attempt in 0..12 {
                let n0 = sim.entities.len();
                sim.request_spawn(
                    &assets,
                    SpawnReq {
                        unit,
                        pos,
                        heading: None,
                        heading_forced: false,
                        owner: usize::MAX,
                        spawner_player: -1,
                        stationary: true,
                        state: None,
                    },
                );
                // request_spawn is synchronous — the entity is already in
                // the list; take the newest match so retries don't alias.
                if let Some(i) = sim.entities[n0..]
                    .iter()
                    .rposition(|e| e.unit == unit && !e.dead)
                    .map(|r| n0 + r)
                {
                    idx = Some(i);
                    break 'spawn;
                }
            }
            let Some(idx) = idx else {
                println!("{uname} @{label}: spawn FAILED");
                continue;
            };
            sim.destroy_entity(&assets, idx, -1);
            // reap sweep runs inside step(); the media spawn queues behind it.
            for _ in 0..3 {
                sim.step(&assets, &PlayerInput::default());
            }
            let spawned = count_media(&sim);
            println!(
                "{uname} @{label}: media_spawns={} {:?}",
                spawned.len().saturating_sub(before),
                spawned
            );
        }
    }

    // Coin release (sub_01ae10 at reap): Twin Gun drops 'slac' x1, but
    // only when the killer is a real player (killer != -1).
    let coin_rid = 0x736c6163u32; // 'slac'
    let count_coins = |sim: &game::sim::Sim| {
        sim.entities
            .iter()
            .filter(|e| !e.dead && e.unit == coin_rid)
            .count()
    };
    for killer in [0i32, -1] {
        let unit = 0x75677774u32; // 'ugwt' Twin Gun
        let pos = Vec2::new(200.0, 240.0);
        sim.scroll = 0.0;
        let mut idx = None;
        'cs: for _ in 0..12 {
            let n0 = sim.entities.len();
            sim.request_spawn(
                &assets,
                SpawnReq {
                    unit,
                    pos,
                    heading: None,
                    heading_forced: false,
                    owner: usize::MAX,
                    spawner_player: -1,
                    stationary: true,
                    state: None,
                },
            );
            if let Some(i) = sim.entities[n0..]
                .iter()
                .rposition(|e| e.unit == unit && !e.dead)
                .map(|r| n0 + r)
            {
                idx = Some(i);
                break 'cs;
            }
        }
        let Some(idx) = idx else {
            println!("Twin Gun: spawn FAILED");
            continue;
        };
        let before = count_coins(&sim);
        sim.destroy_entity(&assets, idx, killer);
        for _ in 0..3 {
            sim.step(&assets, &PlayerInput::default());
        }
        println!(
            "Twin Gun killer={killer}: coin_spawns={}",
            count_coins(&sim).saturating_sub(before)
        );
    }
}

/// Headless sim smoke test: run le01 for `secs` seconds with scripted input.
fn test_sim() {
    use game::sim::{PlayerInput, SlotInput};
    let dir = data_dir();
    let mut assets = assets::GameAssets::open(&dir).expect("open");
    assets.load_all_data();
    let level_id = pak::id("le01");
    let mut sim = game::render::start_level(&mut assets, level_id, 1, 1).expect("level");
    let secs: u32 = std::env::args()
        .position(|a| a == "--secs")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    for tick in 0..secs * 60 {
        // Scripted input: wiggle + periodic fire.
        let t = tick as f32;
        let fire = (tick / 45) % 2 == 0;
        let inp = PlayerInput {
            p1: SlotInput {
                up: (t * 0.05).sin() > 0.3,
                down: (t * 0.05).sin() < -0.3,
                left: (t * 0.03).cos() > 0.5,
                right: (t * 0.03).cos() < -0.5,
                fire,
                bomb: false,
                cycle: false,
            },
            p2: SlotInput::default(),
        };
        sim.advance(&assets, &inp, 1.0 / 60.0);
        if tick % 600 == 599 {
            let p = sim.players[0].as_ref().unwrap();
            let mut projs = 0usize;
            let mut pickups = 0usize;
            let mut pausers = 0usize;
            for e in &sim.entities {
                if e.dead {
                    continue;
                }
                if let Some(d) = assets.units.get(&e.unit) {
                    if d.player_projectile {
                        projs += 1;
                    }
                    if d.pickup_type != pak::NONE_ID {
                        pickups += 1;
                    }
                    if d.states.get(e.state).map(|s| s.pause_vertical_scrolling).unwrap_or(false) {
                        pausers += 1;
                    }
                }
            }
            println!(
                "t={:>4}s scroll={:7.1} ent={:>4} proj={:>3} pick={:>2}/{} pause={} hit={} kill={} ph={:?} p=({:5.1},{:5.1}) sh={:5.1} score={} money={} mult={} lives={} wpn={:08x} done={}",
                tick / 60,
                sim.scroll,
                sim.entities.len(),
                projs,
                pickups,
                sim.stat_pickups,
                pausers,
                sim.stat_hits,
                sim.stat_kills,
                sim.phase,
                p.screen.x,
                p.screen.y,
                p.shields,
                p.score,
                p.money,
                p.multiplier,
                p.lives,
                p.air_weapon,
                sim.level_complete,
            );
        }
        if sim.level_complete && tick % 60 == 0 {
            let p = sim.players[0].as_ref().unwrap();
            println!(
                "  endseq={} pct={} bonus={}/{} rem={} tally(ph={} rem={} purse={}) score={} money={} perfect={} done={}",
                sim.end_seq_state,
                sim.end_bonus_percent,
                sim.end_bonus_total,
                sim.end_bonus_step,
                sim.end_bonus_remaining,
                p.money_tally.phase,
                p.money_tally.remaining,
                p.money_tally.purse,
                p.score,
                p.money,
                sim.perfect_count,
                sim.level_end_complete,
            );
        }
        if sim.level_end_complete {
            println!("LEVEL END COMPLETE at t={}s", tick / 60);
            break;
        }
        if tick > 60 * 400 {
            println!("TIMEOUT waiting for level end at t={}s state={}", tick / 60, sim.end_seq_state);
            break;
        }
    }
    // Final entity census.
    let mut by_unit: std::collections::HashMap<u32, usize> = Default::default();
    for e in &sim.entities {
        *by_unit.entry(e.unit).or_default() += 1;
    }
    println!("final: {} entities, {} distinct units", sim.entities.len(), by_unit.len());
    for (u, n) in by_unit.iter().take(10) {
        println!("  {:08x}: {}", u, n);
    }
}

/// Dump every distinct rule condition/action string used by unit defs.
fn dump_rules() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut conds = std::collections::BTreeSet::new();
    let mut acts = std::collections::BTreeSet::new();
    for u in a.units.values() {
        for st in &u.states {
            for r in &st.rules {
                conds.insert(r.condition.clone());
                acts.insert(r.action.clone());
            }
        }
    }
    println!("=== CONDITIONS ({}):", conds.len());
    for c in &conds { println!("  {:?}", c); }
    println!("=== ACTIONS ({}):", acts.len());
    for c in &acts { println!("  {:?}", c); }
    // Show rule tuples for the rarer conditions.
    let rare = ["Are Fewer", "Number of This", "Animation Has Stopped", "Is Not Active", "Is Tracking"];
    for u in a.units.values() {
        for st in &u.states {
            for r in &st.rules {
                if rare.iter().any(|k| r.condition.contains(k)) {
                    println!("RULE {:08x} '{}' st='{}' unit={:08x} range={} cond={:?} act={:?}",
                        u.id, u.name, st.name, r.unit_id, r.range, r.condition, r.action);
                }
            }
        }
    }
    // Do rule actions resolve to real state names?
    let mut unresolved = std::collections::BTreeSet::new();
    let mut resolved = 0u32;
    for u in a.units.values() {
        for st in &u.states {
            for r in &st.rules {
                let act = r.action.trim();
                if act.is_empty() || act == "none" { continue; }
                let lname = act.to_lowercase();
                if lname == "delete" || lname == "destroy" || lname == "do nothing" { resolved += 1; continue; }
                if u.states.iter().any(|s| s.name.eq_ignore_ascii_case(act)) {
                    resolved += 1;
                } else {
                    unresolved.insert(act.to_string());
                }
            }
        }
    }
    println!("=== rule actions resolving to states: {}, unresolved: {}", resolved, unresolved.len());
    for s in &unresolved { println!("  UNRESOLVED {:?}", s); }
    // Destruct-drop stats across all units.
    let mut coins = 0; let mut bonus = 0; let mut groupcoin = 0; let mut pickups = 0;
    for u in a.units.values() {
        if u.destruct_num_coins > 0 { coins += 1; }
        if u.destruct_release_random_bonus { bonus += 1; }
        if u.destruct_coin_on_group_kill != pak::NONE_ID { groupcoin += 1; }
        if u.pickup_type != pak::NONE_ID { pickups += 1; }
    }
    println!("=== units: coins={} bonus={} groupcoin={} pickups={}", coins, bonus, groupcoin, pickups);
}

fn state_probe(ridv: u32) {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    if let Some(u) = a.units.get(&ridv) {
        for (i, st) in u.states.iter().enumerate() {
            println!(
                "state[{}] '{}' spr={:08x} f={}..{} dirs={} fpd={} timer={}..{} -> '{}' onrange={}.. changeto='{}' entrysnd={:08x}",
                i, st.name, st.sprite_face, st.sprite_frame_min, st.sprite_frame_max,
                st.num_directions, st.frames_per_direction,
                st.on_timer_min, st.on_timer_max,
                st.on_timer_change_to, st.on_range, st.on_range_change_to,
                st.entry_sound.id
            );
        }
    }
}

fn weapon_probe() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut v: Vec<_> = a.weapons.iter().collect();
    v.sort_by_key(|(rid, _)| **rid);
    for (rid, w) in v {
        println!(
            "{:08x} {:10} type={:08x} xoff={} yoff={} face={:08x} frame={} locked={:08x}/{} act={} spawns={} bscale={:.4}",
            rid, w.name, w.weapon_type, w.crosshair_x_offset, w.crosshair_y_offset,
            w.crosshair_face, w.crosshair_frame, w.crosshair_locked_face,
            w.crosshair_locked_frame, w.crosshair_spawn_on_activation,
            w.spawns.len(), w.ballistic_scale
        );
        for s in &w.spawns {
            let u = a.units.get(&s.unit_id);
            println!(
                "    spawn {:08x} ({},{}) sethdg={} ang={} proj={} dmg={:.1} hdg={} tol={} spd={}-{} edtr={}",
                s.unit_id, s.x_loc, s.y_loc, s.set_heading, s.angle,
                u.map(|u| u.player_projectile).unwrap_or(false),
                u.map(|u| u.damage).unwrap_or(-1.0),
                u.map(|u| u.initial_heading).unwrap_or(-1),
                u.map(|u| u.initial_heading_tolerance).unwrap_or(-1),
                u.map(|u| u.initial_speed_min).unwrap_or(-1.0),
                u.map(|u| u.initial_speed_max).unwrap_or(-1.0),
                u.map(|u| u.initial_heading_set_in_editor).unwrap_or(false),
            );
        }
    }
}

#[allow(dead_code)]
fn coin_census() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut n_coin = 0; let mut n_grpcoin = 0; let mut tot_coins = 0i64;
    let mut by_unit: Vec<(ResId, String, i64, ResId)> = Vec::new();
    for (rid, u) in &a.units {
        if u.destruct_coin != pak::NONE_ID && u.destruct_num_coins > 0 {
            n_coin += 1; tot_coins += u.destruct_num_coins;
            by_unit.push((*rid, u.name.clone(), u.destruct_num_coins, u.destruct_coin));
        }
        if u.destruct_coin_on_group_kill != pak::NONE_ID { n_grpcoin += 1; }
    }
    println!("COINS units_with_coin={} total_coins={} units_with_grpcoin={}", n_coin, tot_coins, n_grpcoin);
    by_unit.sort_by_key(|u| -u.2);
    for (rid, name, n, coin) in by_unit.iter().take(25) {
        println!("  {:08x} {:35} n={} -> {:08x}", rid, name, n, coin);
    }
}

#[allow(dead_code)]
fn spawner_census(target: u32) {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut hits = Vec::new();
    for (rid, u) in &a.units {
        if u.destruct_spawn == target { hits.push((rid, u.name.clone(), "destruct_spawn".to_string())); }
        if u.deletion_spawn == target { hits.push((rid, u.name.clone(), "deletion_spawn".to_string())); }
        if u.destruct_coin == target { hits.push((rid, u.name.clone(), "destruct_coin".to_string())); }
        if u.destruct_coin_on_group_kill == target { hits.push((rid, u.name.clone(), "grp_coin".to_string())); }
        for st in &u.states {
            for ss in &st.spawn_sets {
                if ss.spawn_id == target { hits.push((rid, u.name.clone(), "spawnset".to_string())); }
            }
        }
    }
    for (rid, name, how) in &hits { println!("{:08x} {:35} via {}", rid, name, how); }
    println!("total {} spawners of {:08x}", hits.len(), target);
}

#[allow(dead_code)]
fn bonus_census() {
    let dir = data_dir();
    let mut a = assets::GameAssets::open(&dir).expect("open");
    a.load_all_data();
    let mut n = 0;
    for (rid, u) in &a.units {
        if u.destruct_release_random_bonus { n += 1; println!("{:08x} {}", rid, u.name); }
    }
    println!("total {} units with destructReleaseRandomBonus", n);
    // also list the Random Bonus wrapper units and their spawn targets
    for (rid, u) in &a.units {
        if u.name.starts_with("Random Bonus") {
            for st in &u.states {
                for ss in &st.spawn_sets {
                    println!("RB {:08x} {} state '{}' -> {:08x}", rid, u.name, st.name, ss.spawn_id);
                }
            }
        }
    }
}
