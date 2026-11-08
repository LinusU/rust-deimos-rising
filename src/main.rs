mod data;
mod pak;

use pak::{id, PakSet, Tag, NONE_ID};
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
    println!("Deimos Rising remake — run with --dump-data to verify assets.");
}
