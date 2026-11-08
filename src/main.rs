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
fn perm_probe() {
    let dir = data_dir();
    let set = PakSet::open_dir(&dir).expect("open paks");
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
    if args.iter().any(|a| a == "--test-assets") {
        test_assets();
        return;
    }
    if args.iter().any(|a| a == "--perm") {
        perm_probe();
        return;
    }
    if args.iter().any(|a| a == "--pickups") {
        pickups_probe();
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
    // Notice census.
    let mut dn=0; let mut en=0; let mut ed=0;
    for u in a.units.values() {
        if !u.destruct_notice.is_empty() && u.destruct_notice != "none" {dn+=1;}
        if !u.entry_notice.is_empty() && u.entry_notice != "none" {en+=1; if u.entry_notice_delay>0 {ed+=1;}}
    }
    println!("NOTICES destruct={} entry={} entryDelay={}", dn,en,ed);
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
        for (i, line) in s.strings.iter().enumerate().take(12) {
            println!("  [{}] {}", i, line);
        }
    }
    for (rid, l) in &a.levels {
        println!(
            "LEVEL {:08x} name={:?} briefing={:08x} music={:08x} objs={}",
            rid, l.name, l.briefing, l.music, l.objects.len()
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
        println!("  shields {}/{}+{} dmg {} score {} pickup={:08x} destruct_spawn={:08x}", u.shields_base,u.shields_max,u.shields_level_increment,u.damage,u.score,u.pickup_type,u.destruct_spawn);
        for (i,st) in u.states.iter().enumerate() {
            println!("  state[{}] '{}' spr={:08x} dirs={} fpd={} framedly={} maxspd={} delta={} rules={:?}", i, st.name, st.sprite_face, st.num_directions, st.frames_per_direction, st.frame_delay, st.max_speed, st.delta, st.rules.iter().map(|r|(r.name.clone(),r.unit_id,r.range,r.condition.clone(),r.action.clone())).collect::<Vec<_>>());
            for ss in &st.spawn_sets { println!("     spawnset '{}' id={:08x} x={} y={} rate={}-{} volley={}-{} delay={}-{} repeat={}", ss.name, ss.spawn_id, ss.x_offset, ss.y_offset, ss.rate_min, ss.rate_max, ss.volley_min, ss.volley_max, ss.delay_between_min, ss.delay_between_max, ss.repeat_spawns); }
        }
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
            for e in &sim.entities {
                if let Some(d) = assets.units.get(&e.unit) {
                    if d.player_projectile {
                        projs += 1;
                    }
                    if d.pickup_type != pak::NONE_ID {
                        pickups += 1;
                    }
                }
            }
            println!(
                "t={:>4}s scroll={:7.1} ent={:>4} proj={:>3} pick={:>2}/{} hit={} kill={} ph={:?} p=({:5.1},{:5.1}) sh={:5.1} score={} money={} mult={} lives={} wpn={:08x} done={}",
                tick / 60,
                sim.scroll,
                sim.entities.len(),
                projs,
                pickups,
                sim.stat_pickups,
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
        if sim.level_complete {
            println!("LEVEL COMPLETE at t={}s", tick / 60);
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
