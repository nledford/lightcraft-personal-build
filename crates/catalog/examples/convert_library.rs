//! Copy a LightCraft library to the catalog format of another build, or print what this build reads of one
//! (`lightcraft_catalog::convert` has the rules).
//!
//! ```text
//! convert_library to-official <library> <out> [--allow-loss] [--catalog-only]
//! convert_library to-7248     <library> <out> [--allow-loss] [--catalog-only]
//! convert_library dump        <library>
//! ```
//!
//! - `to-official`: a copy official LightCraft (`origin/main`) opens: format 12, one `catalog.snap`, whole records.
//! - `to-7248`: a copy the build `7248a97a77` opens: format 11, photos in segments, compact records.
//! - `dump`: one JSON object per line, what this build's reader makes of the library (for comparing a library with its copy).
//!
//! The library is only read (and must not be open in a program), and `<out>` must be a new folder outside it. Data the
//! target can't hold is listed and the copy is not made, unless `--allow-loss` says to go on without it.
//! `--catalog-only` leaves out the thumbnails, smart previews, originals the library holds and the interop folder.
//!
//! Exit status: 0 done; 2 the copy would lose data and `--allow-loss` was not given (nothing was written); 1 anything
//! else went wrong (nothing was written).

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use lightcraft_catalog::convert::{self, ConvertError, Options, Target};

const USAGE: &str = "usage:
  convert_library to-official <library> <out> [--allow-loss] [--catalog-only]
  convert_library to-7248     <library> <out> [--allow-loss] [--catalog-only]
  convert_library dump        <library>";

/// What was asked.
enum Task {
    Convert { library: PathBuf, out: PathBuf, target: Target, options: Options },
    Dump { library: PathBuf },
}

fn parse(args: &[String]) -> Result<Task, String> {
    let (command, rest) = args.split_first().ok_or("no command")?;
    let mut options = Options::default();
    let mut paths = Vec::new();
    for arg in rest {
        match arg.as_str() {
            "--allow-loss" => options.allow_loss = true,
            "--catalog-only" => options.catalog_only = true,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => paths.push(PathBuf::from(path)),
        }
    }
    let target = match command.as_str() {
        "to-official" => Target::Official,
        "to-7248" => Target::Build7248,
        "dump" => {
            return match <[PathBuf; 1]>::try_from(paths) {
                Ok([library]) if !options.allow_loss && !options.catalog_only => Ok(Task::Dump { library }),
                _ => Err("dump takes one library and no option".to_string()),
            };
        }
        other => return Err(format!("unknown command {other}")),
    };
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
            return ExitCode::from(1);
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
