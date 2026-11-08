//! What showing a library folder costs in a big library (default 1.6 M photos in 30,000 folders,
//! built from a fixed recipe: no real photo, name or path): opening the library on a saved folder
//! view, the first look at it, and switching from one folder to another.
//!
//! ```text
//! cargo run --release -p lightcraft-engine --example library_folder_bench -- <scratch dir> [photos [folders]]
//! ```
//!
//! `<scratch dir>` takes the library (`<dir>/bench-library`, written once and opened by every
//! run, so two builds read the very same file; about 1 GB at the default size). Each timing is
//! three runs, with the machine's load next to it. Peak memory: run the built example under
//! `/usr/bin/time -l` (macOS) / `-v` (Linux).
//!
//! The folders switched between: the one nearest 100 photos, the biggest one, a disk folder that
//! holds about a fifth of the library and one that holds about three quarters.

use std::path::{Path, PathBuf};
use std::time::Instant;

use lightcraft_catalog::{Catalog, Filter, FsStore, Journal, Op, Photo, Sort, Source};
use lightcraft_engine::Session;
use serde_json::json;

type Res<T = ()> = Result<T, Box<dyn std::error::Error>>;

const RUNS: usize = 3;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

/// The machine's load averages, as `uptime` prints them.
fn load() -> String {
    let out = std::process::Command::new("uptime").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    out.and_then(|s| s.split_once("load average").map(|(_, l)| l.trim_start_matches(['s', ':', ' ']).to_string())).unwrap_or_default()
}

fn min(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::INFINITY, f64::min)
}

fn mix(x: u64) -> u64 {
    let x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// One folder of the library: where it is and how many photos are directly in it.
struct Dir {
    path: String,
    photos: u64,
    year: u64,
    month: u64,
}

/// `folders` folders holding `n` photos, in the order their photos come into the library. Sizes
/// fall off as rank^-0.6 (a few big shoots, a long tail of small ones); a fixed shuffle spreads
/// them over three places: three quarters under one disk folder, a fifth under another, the rest
/// on the system disk.
fn shape(n: u64, folders: u64) -> Vec<Dir> {
    let weights: Vec<f64> = (1..=folders).map(|r| (r as f64).powf(-0.6)).collect();
    let total: f64 = weights.iter().sum();
    let mut sizes: Vec<u64> = weights.iter().map(|w| (w / total * n as f64).round().max(1.0) as u64).collect();
    let sum: u64 = sizes.iter().sum();
    if let Some(biggest) = sizes.first_mut() {
        *biggest = (*biggest + n).saturating_sub(sum).max(1);
    }
    let mut order: Vec<u64> = (0..folders).collect();
    order.sort_by_key(|r| mix(*r));
    let mut done = 0u64;
    let mut dirs = Vec::with_capacity(order.len());
    for r in order {
        let photos = sizes[r as usize];
        let (year, month) = (2000 + mix(r) % 25, 1 + mix(r + 7) % 12);
        let share = done as f64 / n as f64;
        let path = if share < 0.74 {
            format!("/Volumes/Photos/Archive/{year}/{year}-{month:02}/Shoot {r:05}")
        } else if share < 0.92 {
            format!("/Volumes/Photos/Phone/{year}/{year}-{month:02}/Day {r:05}")
        } else {
            format!("/Users/someone/Pictures/{year}/Folder {r:05}")
        };
        done += photos;
        dirs.push(Dir { path, photos, year, month });
    }
    dirs
}

fn build(dirs: &[Dir]) -> Res<Catalog> {
    let mut cat = Catalog::new();
    for d in dirs {
        for _ in 0..d.photos {
            let id = cat.alloc_photo_id();
            let name = format!("IMG_{:08}.jpg", id.0);
            let path = format!("{}/{name}", d.path);
            let mut p = Photo::new(id, Source::File { path }, &name, "JPEG", 4032, 3024, "2026-09-30T12:00:00");
            p.file_size = 3_400_000 + id.0;
            p.captured = Some(format!("{}-{:02}-{:02}T10:{:02}:{:02}", d.year, d.month, 1 + id.0 % 28, id.0 % 60, (id.0 / 60) % 60));
            p.content_hash = Some(format!("{:032x}", id.0.wrapping_mul(0x9e37_79b9_7f4a_7c15)));
            cat.apply(Op::AddPhoto { photo: Box::new(p) })?;
        }
    }
    Ok(cat)
}

/// The photos in `folder` or below it, by the recipe.
fn photos_under(dirs: &[Dir], folder: &str) -> u64 {
    dirs.iter().filter(|d| d.path.starts_with(folder) && d.path[folder.len()..].starts_with('/')).map(|d| d.photos).sum()
}

/// The id of the first photo of the folder `path`: ids are handed out in the recipe's order.
fn first_id(dirs: &[Dir], path: &str) -> u64 {
    1 + dirs.iter().take_while(|d| d.path != path).map(|d| d.photos).sum::<u64>()
}

fn write_view(lib: &Path, view: &serde_json::Value) -> Res {
    std::fs::write(lib.join("view.json"), serde_json::to_vec(view)?)?;
    Ok(())
}

fn main() -> Res {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().ok_or("usage: library_folder_bench <scratch dir> [photos [folders]]")?);
    let n: u64 = args.next().map(|a| a.parse()).transpose()?.unwrap_or(1_600_000);
    let folders: u64 = args.next().map(|a| a.parse()).transpose()?.unwrap_or(30_000);
    if dir.canonicalize().unwrap_or_else(|_| dir.clone()).components().any(|c| c.as_os_str() == "Pictures") {
        return Err("the scratch dir is under a Pictures folder: pick a scratch place".into());
    }
    let lib = dir.join("bench-library");
    let recipe = format!("{n} photos in {folders} folders");
    let dirs = shape(n, folders);
    let small = dirs.iter().min_by_key(|d| d.photos.abs_diff(100)).ok_or("no folders")?;
    let biggest = dirs.iter().max_by_key(|d| d.photos).ok_or("no folders")?;
    let (phone, archive) = ("/Volumes/Photos/Phone", "/Volumes/Photos/Archive");
    // (the first folder is the one the saved view shows: the switches start with the second)
    let targets = [
        (format!("{} photos", small.photos), small.path.clone()),
        (format!("{} photos (the biggest)", biggest.photos), biggest.path.clone()),
        (format!("{} photos (a disk folder)", photos_under(&dirs, phone)), phone.to_string()),
        (format!("{} photos (a disk folder)", photos_under(&dirs, archive)), archive.to_string()),
    ];
    println!("library: {recipe}; folders switched between: {}", targets.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(" | "));

    if std::fs::read_to_string(lib.join("recipe.txt")).ok().as_deref() != Some(recipe.as_str()) {
        if lib.exists() {
            return Err(format!("{} holds another library: remove it first", lib.display()).into());
        }
        let t = Instant::now();
        let cat = build(&dirs)?;
        println!("built in {:.1} s", t.elapsed().as_secs_f64());
        let t = Instant::now();
        let (mut journal, _, _) = Journal::open(Box::new(FsStore::open(&lib)?))?;
        journal.snapshot(&cat)?;
        println!("snapshot written in {:.1} s ({} MB)", t.elapsed().as_secs_f64(), std::fs::metadata(lib.join("catalog.snap"))?.len() / 1_000_000);
        drop((journal, cat));
        std::fs::write(lib.join("recipe.txt"), &recipe)?;
    }

    // Journal::open alone: what opening costs whatever the view shows
    let mut journal = Vec::new();
    for _ in 0..RUNS {
        let t = Instant::now();
        let opened = Journal::open(Box::new(FsStore::open(&lib)?))?;
        journal.push(ms(t));
        drop(opened);
    }
    println!("Journal::open alone: {:.0} ms min of {journal:.0?} (load {})", min(&journal), load());

    let first = first_id(&dirs, &small.path);
    let saved_folder = json!({"source": {"kind": "libraryFolder"}, "library_folder": small.path, "selection": {"ids": [first], "active": first}});
    let saved_all = json!({"source": {"kind": "all"}, "selection": {"ids": [first], "active": first}});
    for (what, view) in [("All Photos (the control)", &saved_all), ("a folder of 100 photos", &saved_folder)] {
        let mut opens = Vec::new();
        let mut session: Option<Session> = None;
        for run in 0..RUNS {
            drop(session.take());
            write_view(&lib, view)?;
            let t = Instant::now();
            let mut s = Session::new().with_fs();
            s.open_library(&lib, false)?;
            let open = ms(t);
            let t = Instant::now();
            let shown = s.visible().len();
            println!("open on {what}: run {run}: open_library {open:.0} ms, then the first list ({shown} photos) {:.1} ms (load {})", ms(t), load());
            opens.push(open);
            session = Some(s);
        }
        println!("open on {what}: open_library {:.0} ms min of {opens:.0?}", min(&opens));
        let Some(mut s) = session else { continue };
        if view == &saved_all {
            // the first switch to a folder: nothing derived from the folders exists yet
            let t = Instant::now();
            s.execute("library.source", &json!({"kind": "libraryFolder", "path": small.path}))?;
            println!("the first switch to a folder after opening on All Photos: {:.0} ms (load {})", ms(t), load());
            continue;
        }
        // switching: the folders in turn, from the second (the first is shown), three rounds
        let mut switch = vec![Vec::new(); targets.len()];
        let mut list = vec![Vec::new(); targets.len()];
        let mut query = vec![Vec::new(); targets.len()];
        for k in 1..=RUNS * targets.len() {
            let at = k % targets.len();
            let path = &targets[at].1;
            let t = Instant::now();
            let r = s.execute("library.source", &json!({"kind": "libraryFolder", "path": path}))?;
            switch[at].push(ms(t));
            let t = Instant::now();
            let shown = s.visible().len();
            list[at].push(ms(t));
            if r["count"].as_u64() != Some(shown as u64) {
                return Err(format!("{path}: library.source says {} photos, the list has {shown}", r["count"]).into());
            }
            let t = Instant::now();
            let f = Filter { library_folder: Some(path.clone()), ..Default::default() };
            let _ = s.catalog.query(&f, &Sort::default());
            query[at].push(ms(t));
        }
        for (at, (label, _)) in targets.iter().enumerate() {
            println!(
                "switch to a folder of {label}: library.source {:.0} ms min of {:.0?}; the list after it {:.1} ms; Catalog::query alone {:.0} ms min of {:.0?}",
                min(&switch[at]),
                switch[at],
                min(&list[at]),
                min(&query[at]),
                query[at]
            );
        }
        println!("(load {})", load());
    }
    Ok(())
}
