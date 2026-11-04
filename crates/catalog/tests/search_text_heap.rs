//! What the free-text search's kept text costs in memory (dhat's testing mode). A big library's text is one block and a few
//! tables, not a box and a table entry for each photo: at 1.7 million photos those cost 90 MiB more than the text itself.
//! One test per binary (dhat's profiler is global).

use lightcraft_catalog::{Catalog, Op, Photo, PhotoId, Source};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Where `Catalog::prebuild_search_text` starts to make the text.
const PHOTOS: usize = 50_000;

fn library() -> Catalog {
    let mut c = Catalog::new();
    let ops: Vec<Op> = (0..PHOTOS)
        .map(|i| {
            let mut p =
                Photo::new(PhotoId(i as u64 + 1), Source::Demo { scene: 1 }, &format!("IMG_{i:06}.jpg"), "JPEG", 60, 40, "2026-01-01T00:00:00");
            p.meta.title = format!("Frame {i}");
            p.meta.keywords = vec![format!("kw{}", i % 500), "Travel|Italy".into()];
            Op::AddPhoto { photo: Box::new(p) }
        })
        .collect();
    c.apply(Op::Batch { ops }).unwrap();
    c
}

/// The length of the text of every photo: its seven fields and each keyword, with a NUL between every two (all ASCII, so
/// lower-casing doesn't change it).
fn text_len(c: &Catalog) -> usize {
    c.photos()
        .map(|p| {
            let fields = [&p.file_name, &p.meta.title, &p.meta.caption, &p.meta.camera, &p.meta.lens, &p.meta.location, &p.format];
            let words = fields.iter().map(|f| f.len()).chain(p.meta.keywords.iter().map(String::len)).sum::<usize>();
            words + fields.len() + p.meta.keywords.len() - 1
        })
        .sum()
}

#[test]
fn the_kept_search_text_is_one_block_not_a_box_for_each_photo() {
    let c = library();
    let text = text_len(&c);
    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();
    c.prebuild_search_text();
    let by = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while !c.search_text_ready() {
        assert!(std::time::Instant::now() < by, "the text was not made in two minutes");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let after = dhat::HeapStats::get();
    let blocks = after.curr_blocks.saturating_sub(before.curr_blocks);
    let bytes = after.curr_bytes.saturating_sub(before.curr_bytes);
    let besides = bytes.saturating_sub(text) / PHOTOS;
    eprintln!("kept text of {PHOTOS} photos: {blocks} blocks, {bytes} bytes for {text} bytes of text ({besides} bytes a photo besides it)");
    // (the catalog's own photos were made before the profiler started, so what is counted is the text and what finds it)
    assert!(blocks <= 8, "{blocks} blocks kept: one for each photo?");
    assert!(bytes <= text + 24 * PHOTOS + 4096, "{bytes} bytes kept for {text} bytes of text: {besides} a photo besides it");
}
