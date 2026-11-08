//! What a target has no place for, and clearing it from the copy.
//!
//! Every field of this build's catalog that a target lacks is named once, in [`DROPS`], with what
//! its loss costs; [`for_target`] clears it from a copy of the catalog and counts the records that
//! held something there. A field this build adds is classified for both targets, or the test
//! `tests_convert::every_field_is_classified` fails: it keeps the list of the fields of a fully
//! set photo.

use std::collections::BTreeMap;
use std::sync::Arc;

use lightcraft_develop::{LensSource, Process};

use super::{Loss, Target};
use crate::{Album, AlbumId, Catalog, Filter, FolderRecord, LeftInFile, Photo};

/// The newest rendering process build 7248 has (it keeps a number it doesn't know, and renders it as its newest).
const NEWEST_PROCESS_7248: Process = Process::V2;

/// Who can't hold a kind of data.
#[derive(Clone, Copy)]
enum Lost {
    /// Official LightCraft and build 7248 both lack it.
    Both,
    /// Only official LightCraft lacks it (build 7248 has it).
    Official,
    /// Only build 7248 lacks it (official LightCraft has it).
    Build7248,
}

/// A kind of data a target can't hold.
struct Drop {
    id: &'static str,
    on: Lost,
    what: &'static str,
    effect: &'static str,
}

impl Drop {
    fn lost_on(&self, target: Target) -> bool {
        matches!((self.on, target), (Lost::Both, _) | (Lost::Official, Target::Official) | (Lost::Build7248, Target::Build7248))
    }
}

const MADE: &str = "folder.made";
const MAKE: &str = "meta.make";
const LENS_MAKE: &str = "meta.lens_make";
const FLASH: &str = "meta.flash";
const EXPOSURE_BIAS: &str = "meta.exposure_bias";
const SERIAL_NUMBER: &str = "meta.serial_number";
const LENS_SOURCE: &str = "embedded_lens.source";
const NR_OUTLIERS: &str = "settings.detail.nr_outliers";
const ON_PHOTO: &str = "settings.masks.on_photo";
const TRUNCATED: &str = "photo.truncated";
const LEFT_IN_FILE: &str = "photo.left_in_file";
const RULE_ALBUMS: &str = "album.rules.albums";
const RULE_DATES: &str = "album.rules.dates";
const RULE_LIBRARY_FOLDERS: &str = "album.rules.library_folders";
const RULE_KEYWORDS: &str = "album.rules.keywords";

const DROPS: &[Drop] = &[
    Drop {
        id: MADE,
        on: Lost::Both,
        what: "a folder record's `made`: a folder created through LightCraft before it holds a photo",
        effect: "the folder has no row of its own until a photo's path is in it (an empty one is gone from the tree); \
                 its color label is kept, and a record that said only this is dropped; the folder on disk is not touched",
    },
    Drop {
        id: MAKE,
        on: Lost::Build7248,
        what: "a photo's camera maker (`meta.make`)",
        effect: "build 7248 has no field for it (the file still has it)",
    },
    Drop {
        id: LENS_MAKE,
        on: Lost::Build7248,
        what: "a photo's lens maker (`meta.lens_make`)",
        effect: "build 7248 has no field for it (the file still has it)",
    },
    Drop {
        id: FLASH,
        on: Lost::Build7248,
        what: "whether the flash fired (`meta.flash`)",
        effect: "build 7248 has no field for it (the file still has it)",
    },
    Drop {
        id: EXPOSURE_BIAS,
        on: Lost::Build7248,
        what: "the exposure compensation (`meta.exposure_bias`)",
        effect: "build 7248 has no field for it (the file still has it)",
    },
    Drop {
        id: SERIAL_NUMBER,
        on: Lost::Build7248,
        what: "the camera's serial number (`meta.serial_number`)",
        effect: "build 7248 has no field for it (the file still has it)",
    },
    Drop {
        id: LENS_SOURCE,
        on: Lost::Build7248,
        what: "where an embedded lens correction came from (`embedded_lens.source`: a lens database profile)",
        effect: "build 7248 reads the correction as the file's own; the correction itself is kept",
    },
    Drop {
        id: NR_OUTLIERS,
        on: Lost::Build7248,
        what: "the develop setting `detail.nr_outliers` (outlier photosites replaced before demosaicing), in the photo's edit, versions or history",
        effect: "build 7248 doesn't have the setting: the photo renders without it, as it did before the setting existed",
    },
    Drop {
        id: ON_PHOTO,
        on: Lost::Build7248,
        what: "a mask's `on_photo` (its coordinates are on the photo as shot, not on the corrected frame), in the photo's edit, versions or history",
        effect: "build 7248 reads every mask's coordinates as on the corrected frame: a mask stays where it is while no lens correction, Upright, \
                 Transform or Rotate moves the frame, and sits elsewhere when one does",
    },
    Drop {
        id: TRUNCATED,
        on: Lost::Official,
        what: "a photo's mark of a JPEG cut short (`truncated`: shown from the data present, \"Damaged file\")",
        effect: "official LightCraft has no field for it, so the copy doesn't say the file is cut short (the file on disk is not touched); \
                 official LightCraft refuses such a file as unreadable",
    },
    Drop {
        id: LEFT_IN_FILE,
        on: Lost::Official,
        what: "a photo's note of what its file says that import left in the file (`left_in_file`: keywords, rating/flag/label, descriptions, location, develop settings)",
        effect: "official LightCraft has no field for it, so it treats the file's keywords, ratings and labels, descriptions, location or settings that were \
                 never read into the library as nothing, and a sidecar it writes for the photo can take them out of the file",
    },
    Drop {
        id: RULE_ALBUMS,
        on: Lost::Official,
        what: "the albums a smart album's rules choose (`albums` beside `album`)",
        effect: "official LightCraft has no such rule: the smart album keeps its other rules and matches more photos than it did",
    },
    Drop {
        id: RULE_DATES,
        on: Lost::Official,
        what: "the dates a smart album's rules choose (`dates` beside `date`)",
        effect: "official LightCraft has no such rule: the smart album keeps its other rules and matches more photos than it did",
    },
    Drop {
        id: RULE_LIBRARY_FOLDERS,
        on: Lost::Official,
        what: "the library folders a smart album's rules choose (`libraryFolders` beside `libraryFolder`)",
        effect: "official LightCraft has no such rule: the smart album keeps its other rules and matches more photos than it did",
    },
    Drop {
        id: RULE_KEYWORDS,
        on: Lost::Official,
        what: "the keywords a smart album's rules choose (`keywords` beside `keyword`)",
        effect: "official LightCraft has no such rule: the smart album keeps its other rules and matches more photos than it did",
    },
];

/// The catalog a target is written from, and what was cleared to make it.
pub(crate) struct Stripped {
    pub catalog: Catalog,
    pub losses: Vec<Loss>,
    /// What reads differently without being lost.
    pub warnings: Vec<String>,
}

/// `source` as `target` can hold it: a copy with everything the target lacks cleared, and the records that held some of it counted.
pub(crate) fn for_target(source: &Catalog, target: Target) -> Stripped {
    let mut catalog = source.clone();
    let mut tally: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut warnings = Vec::new();
    catalog.folder_records = folder_records(&source.folder_records, &mut tally);
    if target == Target::Official {
        catalog.albums = album_rules(&source.albums, &mut tally);
    }
    let mut newer_process = 0u64;
    for (id, photo) in &source.photos {
        if target == Target::Build7248 {
            newer_process += u64::from(photo.settings_holders().any(|s| s.process > NEWEST_PROCESS_7248.version()));
        }
        let found = found_in(photo, target);
        if found.is_empty() {
            continue;
        }
        for what in found {
            *tally.entry(what).or_default() += 1;
        }
        let mut cleared = Photo::clone(photo);
        clear(&mut cleared, target);
        catalog.photos.insert(*id, Arc::new(cleared));
    }
    if newer_process > 0 {
        warnings.push(format!(
            "{newer_process} photo(s) have settings on a rendering process newer than build 7248's newest (2): it keeps the number and renders them as process 2, \
             so sharpening, Contrast and Blacks, color noise reduction and highlight roll-off look different there; nothing is dropped"
        ));
    }
    let losses = tally
        .into_iter()
        .filter_map(|(id, records)| {
            let d = DROPS.iter().find(|d| d.id == id && d.lost_on(target))?;
            Some(Loss { id: d.id.to_string(), what: d.what.to_string(), records, effect: d.effect.to_string() })
        })
        .collect();
    Stripped { catalog, losses, warnings }
}

/// The folder records of a target that holds no `made`: a record that said nothing else is not kept.
fn folder_records(records: &BTreeMap<String, FolderRecord>, tally: &mut BTreeMap<&'static str, u64>) -> BTreeMap<String, FolderRecord> {
    let mut kept = BTreeMap::new();
    for (key, record) in records {
        // (every field by name: a new one doesn't compile until it is classified here)
        let FolderRecord { label, made } = record;
        if made.is_some() {
            *tally.entry(MADE).or_default() += 1;
        }
        if label.is_some() {
            kept.insert(key.clone(), FolderRecord { label: *label, made: None });
        }
    }
    kept
}

/// The smart albums of a target that has no place for the rule fields this build added: each one found is counted and cleared
/// (the album then matches more photos than it did).
fn album_rules(albums: &BTreeMap<AlbumId, Album>, tally: &mut BTreeMap<&'static str, u64>) -> BTreeMap<AlbumId, Album> {
    let mut kept = albums.clone();
    for album in kept.values_mut() {
        let Some(rules) = album.smart.as_mut() else { continue };
        // (every field by name: a new one doesn't compile until it is classified here and in the field list)
        let Filter {
            text: _,
            rating: _,
            rating_op: _,
            flag: _,
            label: _,
            only: _,
            only_none: _,
            labels: _,
            flags: _,
            kind: _,
            edited: _,
            album: _,
            albums,
            deleted: _,
            date: _,
            dates,
            library_folder: _,
            library_folders,
            keyword: _,
            keywords,
            person: _,
            camera: _,
            lens: _,
            date_from: _,
            date_to: _,
            imported: _,
            imported_from: _,
            merged: _,
            folder: _,
            subfolders: _,
            rule_set: _,
        } = rules.as_mut();
        for (set, id) in [
            (!albums.is_empty(), RULE_ALBUMS),
            (!dates.is_empty(), RULE_DATES),
            (!library_folders.is_empty(), RULE_LIBRARY_FOLDERS),
            (!keywords.is_empty(), RULE_KEYWORDS),
        ] {
            if set {
                *tally.entry(id).or_default() += 1;
            }
        }
        albums.clear();
        dates.clear();
        library_folders.clear();
        keywords.clear();
    }
    kept
}

/// What `target` can't hold of `p`, by [`DROPS`] id.
pub(crate) fn found_in(p: &Photo, target: Target) -> Vec<&'static str> {
    let m = &p.meta;
    let mut found = Vec::new();
    let mut note = |has: bool, id: &'static str| {
        if has && DROPS.iter().any(|d| d.id == id && d.lost_on(target)) {
            found.push(id);
        }
    };
    note(!m.make.is_empty(), MAKE);
    note(!m.lens_make.is_empty(), LENS_MAKE);
    note(m.flash.is_some(), FLASH);
    note(m.exposure_bias.is_some(), EXPOSURE_BIAS);
    note(!m.serial_number.is_empty(), SERIAL_NUMBER);
    note(p.embedded_lens.as_ref().is_some_and(|l| !l.source.is_file()), LENS_SOURCE);
    note(p.settings_holders().any(|s| s.detail.nr_outliers), NR_OUTLIERS);
    note(p.settings_holders().any(|s| s.masks.iter().any(|m| m.on_photo)), ON_PHOTO);
    note(p.truncated, TRUNCATED);
    note(!p.left_in_file.is_nothing(), LEFT_IN_FILE);
    found
}

/// `p` without what [`found_in`] finds for `target`.
pub(crate) fn clear(p: &mut Photo, target: Target) {
    if target == Target::Official {
        p.truncated = false;
        p.left_in_file = LeftInFile::default();
        return;
    }
    let m = &mut p.meta;
    m.make.clear();
    m.lens_make.clear();
    m.flash = None;
    m.exposure_bias = None;
    m.serial_number.clear();
    if let Some(lens) = p.embedded_lens.as_mut() {
        lens.source = LensSource::File;
    }
    for settings in p.settings_holders_mut() {
        if settings.detail.nr_outliers || settings.masks.iter().any(|m| m.on_photo) {
            // (the settings may be shared with other photos, and with the library this was read from: only this photo's are changed)
            let settings = Arc::make_mut(settings);
            settings.detail.nr_outliers = false;
            for mask in &mut settings.masks {
                mask.on_photo = false;
            }
        }
    }
}
