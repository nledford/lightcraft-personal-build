//! What the sidebar's tally of the whole library ([`LibraryTally`]) keeps in memory per photo (dhat's testing mode). It keeps a
//! record of what each photo added so an edit to a few photos can be followed; the record holds numbers for the folder, the day
//! and the keyword levels the photo shares with others, not a string and a box of strings of its own (that was 130 bytes a photo
//! and a heap block for every photo with a keyword; at 1.7 million photos, 200 MiB). One test per binary (dhat's profiler is global).

use lightcraft_catalog::library_tally::LibraryTally;
use lightcraft_catalog::{Catalog, Op, Photo, PhotoId, Source};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const PHOTOS: usize = 50_000;

/// A library of folders (some with names that aren't ASCII), days, and keywords nested up to three levels deep (half of the
/// photos have none).
fn library() -> Catalog {
    let mut c = Catalog::new();
    let ops: Vec<Op> = (0..PHOTOS)
        .map(|i| {
            let dir =
                if i % 7 == 0 { format!("/Volumes/nas/Fotos/Ünïcode {}", i % 90) } else { format!("/Volumes/nas/Photos/2026/job-{}", i % 300) };
            let mut p = Photo::new(
                PhotoId(i as u64 + 1),
                Source::File { path: format!("{dir}/IMG_{i:06}.jpg") },
                &format!("IMG_{i:06}.jpg"),
                "JPEG",
                60,
                40,
                "2026-01-01T00:00:00",
            );
            p.captured = Some(format!("2026-{:02}-{:02}T10:00:00", 1 + i % 12, 1 + i % 28));
            p.meta.keywords = match i % 4 {
                0 | 1 => Vec::new(),
                2 => vec![format!("Travel|Italy|City{}", i % 40)],
                _ => vec![format!("People|Friend{}", i % 25), "Travel|Italy".into()],
            };
            Op::AddPhoto { photo: Box::new(p) }
        })
        .collect();
    c.apply(Op::Batch { ops }).unwrap();
    c
}

#[test]
fn the_tally_keeps_a_few_bytes_for_each_photo_not_strings_and_boxes() {
    let c = library();
    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();
    let tally = LibraryTally::of(&c);
    let after = dhat::HeapStats::get();
    let blocks = after.curr_blocks.saturating_sub(before.curr_blocks);
    let bytes = after.curr_bytes.saturating_sub(before.curr_bytes);
    let each = bytes / PHOTOS;
    eprintln!("tally of {PHOTOS} photos: {blocks} blocks, {bytes} bytes ({each} bytes a photo)");
    assert_eq!(tally.counts().total, PHOTOS);
    // (a slot of 16 bytes for each photo, and what is shared: a few hundred folders and days and a few dozen keyword lists)
    assert!(bytes <= 24 * PHOTOS, "{bytes} bytes kept ({each} a photo, {blocks} blocks): a string or a box for each photo?");
    // (a block for every photo with a keyword was 25,000 of them)
    assert!(blocks <= 2_000, "{blocks} blocks kept: a box for each photo?");
}
