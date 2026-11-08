//! Copy a LightCraft library to the catalog format of another build, or print what this build reads of one
//! (`lightcraft_catalog::convert` has the rules).
//!
//! ```text
//! convert_library to-official <library> <out> [--allow-loss] [--catalog-only]
//! convert_library to-7248     <library> <out> [--allow-loss] [--catalog-only]
//! convert_library dump        <library>
//! convert_library census      <library> <out.json>
//! convert_library census-diff <a.json> <b.json> [--counts-only]
//! ```
//!
//! - `to-official`: a copy official LightCraft (`origin/main`) opens: format 12, one `catalog.snap`, whole records.
//! - `to-7248`: a copy the build `7248a97a77` opens: format 11, photos in segments, compact records.
//! - `dump`: one JSON object per line, what this build's reader makes of the library (for comparing a library with its copy).
//! - `census`: a read-only manifest (JSON) of what the library holds: per photo and folder the color label and keywords, and
//!   rating, flag, albums, stack, edited and a hash of the develop settings. It opens nothing for writing in the library,
//!   takes no lock, refuses an `<out.json>` in the library or already there, and prints counts only.
//! - `census-diff`: what two censuses differ in. The photos and folders whose label or keywords changed, appeared or
//!   disappeared come first; then the other fields, counted. `--counts-only` prints kinds and counts and no name (for a
//!   library whose names must not be shown).
//!
//! The library is only read (a conversion also needs it closed), and `<out>` must be a new folder outside it. Data the
//! target can't hold is listed and the copy is not made, unless `--allow-loss` says to go on without it.
//! `--catalog-only` leaves out the thumbnails, smart previews, originals the library holds and the interop folder.
//!
//! Exit status: 0 done; 2 the copy would lose data and `--allow-loss` was not given (nothing was written); 1 anything
//! else went wrong (nothing was written). `census-diff`: 0 no difference in the labels and keywords of photos and folders
//! (or in which photos and folders there are, or in the label names and the keyword list), 1 there is one, 2 a census could
//! not be read or the command was wrong.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lightcraft_catalog::convert::{self, ConvertError, Options, Target, census};

const USAGE: &str = "usage:
  convert_library to-official <library> <out> [--allow-loss] [--catalog-only]
  convert_library to-7248     <library> <out> [--allow-loss] [--catalog-only]
  convert_library dump        <library>
  convert_library census      <library> <out.json>
  convert_library census-diff <a.json> <b.json> [--counts-only]";

/// What was asked.
enum Task {
    Convert { library: PathBuf, out: PathBuf, target: Target, options: Options },
    Dump { library: PathBuf },
    Census { library: PathBuf, out: PathBuf },
    Diff { a: PathBuf, b: PathBuf, counts_only: bool },
}

fn parse(args: &[String]) -> Result<Task, String> {
    let (command, rest) = args.split_first().ok_or("no command")?;
    let mut options = Options::default();
    let mut counts_only = false;
    let mut paths = Vec::new();
    for arg in rest {
        match arg.as_str() {
            "--allow-loss" => options.allow_loss = true,
            "--catalog-only" => options.catalog_only = true,
            "--counts-only" => counts_only = true,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => paths.push(PathBuf::from(path)),
        }
    }
    let convert_options = options.allow_loss || options.catalog_only;
    let target = match command.as_str() {
        "to-official" => Target::Official,
        "to-7248" => Target::Build7248,
        "dump" => {
            return match <[PathBuf; 1]>::try_from(paths) {
                Ok([library]) if !convert_options && !counts_only => Ok(Task::Dump { library }),
                _ => Err("dump takes one library and no option".to_string()),
            };
        }
        "census" => {
            return match <[PathBuf; 2]>::try_from(paths) {
                Ok([library, out]) if !convert_options && !counts_only => Ok(Task::Census { library, out }),
                _ => Err("census takes a library and the new file for the census, and no option".to_string()),
            };
        }
        "census-diff" => {
            return match <[PathBuf; 2]>::try_from(paths) {
                Ok([a, b]) if !convert_options => Ok(Task::Diff { a, b, counts_only }),
                _ => Err("census-diff takes two census files and only --counts-only".to_string()),
            };
        }
        other => return Err(format!("unknown command {other}")),
    };
    if counts_only {
        return Err(format!("{command} takes no --counts-only"));
    }
    match <[PathBuf; 2]>::try_from(paths) {
        Ok([library, out]) => Ok(Task::Convert { library, out, target, options }),
        Err(_) => Err(format!("{command} takes a library and the new folder for the copy")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let task = match parse(&args) {
        Ok(task) => task,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "{e}\n{USAGE}");
            // (`census-diff` exits 1 for a difference: a mistake in how it was called is not one)
            return ExitCode::from(if args.first().is_some_and(|command| command == "census-diff") { 2 } else { 1 });
        }
    };
    let done = match task {
        Task::Convert { library, out, target, options } => convert::convert(&library, &out, target, options).map(|report| {
            let _ = write!(std::io::stdout().lock(), "{report}");
        }),
        Task::Dump { library } => {
            let mut out = BufWriter::new(std::io::stdout().lock());
            convert::dump(&library, &mut out).and_then(|()| out.flush().map_err(|e| ConvertError::Io(format!("can't write the dump: {e}"))))
        }
        Task::Census { library, out } => census::write(&library, &out).map(|summary| {
            let _ = write!(std::io::stdout().lock(), "{summary}");
        }),
        Task::Diff { a, b, counts_only } => return compare(&a, &b, counts_only),
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(ConvertError::Losses(report)) => {
            let mut err = std::io::stderr().lock();
            let _ = write!(err, "{report}");
            let _ = writeln!(
                err,
                "refused: {} kind(s) of data would be lost; nothing was written. Run again with --allow-loss to copy without them.",
                report.losses.len()
            );
            ExitCode::from(2)
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "error: {e}");
            ExitCode::from(1)
        }
    }
}

/// `census-diff`: 0 no difference of what must be kept, 1 there is one, 2 a census could not be read.
fn compare(a: &Path, b: &Path, counts_only: bool) -> ExitCode {
    match census::diff_files(a, b) {
        Ok(diff) => {
            let _ = write!(std::io::stdout().lock(), "{}", diff.report(counts_only));
            if diff.differs() { ExitCode::from(1) } else { ExitCode::SUCCESS }
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "error: {e}");
            ExitCode::from(2)
        }
    }
}
