use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::process::ExitCode;

/// Files at or above this size are streamed through `bluecsv::stream` instead
/// of being read fully into memory. Matches the LSP `maxBufferBytes` default.
const DEFAULT_STREAM_THRESHOLD: u64 = 10 * 1024 * 1024;

#[derive(Copy, Clone)]
enum StreamMode {
    /// Decide at dispatch time based on file size (or buffer for stdin).
    Auto,
    Force,
    Never,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    // Peel off --stream / --no-stream regardless of position so `bluecsv
    // --stream align file.csv` and `bluecsv align --stream file.csv` both
    // work.
    let (mode, rest) = parse_stream_flag(&args);
    let argv: Vec<&str> = rest.iter().map(String::as_str).collect();

    match argv.as_slice() {
        [_, "align", path] => run_align(path, mode),
        [_, "unalign", path] => run_unalign(path, mode),
        [_, "infer", path] => run_infer(path),
        [_, "stats", path, col] => run_stats(path, col),
        _ => {
            eprintln!(
                "usage:\n  bluecsv [--stream|--no-stream] align <path|->\n  bluecsv [--stream|--no-stream] unalign <path|->\n  bluecsv infer <path|->\n  bluecsv stats <path|-> <column-index>\n\nstreaming auto-detects at {DEFAULT_STREAM_THRESHOLD} bytes; override with BLUECSV_STREAM_THRESHOLD."
            );
            ExitCode::from(2)
        }
    }
}

fn parse_stream_flag(args: &[String]) -> (StreamMode, Vec<String>) {
    let mut mode = StreamMode::Auto;
    let mut rest = Vec::with_capacity(args.len());
    for a in args {
        match a.as_str() {
            "--stream" => mode = StreamMode::Force,
            "--no-stream" => mode = StreamMode::Never,
            _ => rest.push(a.clone()),
        }
    }
    (mode, rest)
}

fn stream_threshold() -> u64 {
    env::var("BLUECSV_STREAM_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_STREAM_THRESHOLD)
}

/// Decide whether a path should be streamed. Stdin (`-`) is always buffered
/// because a non-seekable source can't support the two-pass align.
fn should_stream(path: &str, mode: StreamMode) -> bool {
    if path == "-" {
        return false;
    }
    match mode {
        StreamMode::Force => true,
        StreamMode::Never => false,
        StreamMode::Auto => fs::metadata(path)
            .map(|m| m.len() >= stream_threshold())
            .unwrap_or(false),
    }
}

fn run_align(path: &str, mode: StreamMode) -> ExitCode {
    if should_stream(path, mode) {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("bluecsv: {path}: {e}");
                return ExitCode::from(1);
            }
        };
        let stdout = io::stdout().lock();
        if let Err(e) = bluecsv::stream::stream_align(file, stdout) {
            eprintln!("bluecsv: align: {e}");
            return ExitCode::from(1);
        }
        ExitCode::SUCCESS
    } else {
        run_buffered(path, bluecsv::align)
    }
}

fn run_unalign(path: &str, mode: StreamMode) -> ExitCode {
    if should_stream(path, mode) {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("bluecsv: {path}: {e}");
                return ExitCode::from(1);
            }
        };
        let stdout = io::stdout().lock();
        if let Err(e) = bluecsv::stream::stream_unalign(file, stdout) {
            eprintln!("bluecsv: unalign: {e}");
            return ExitCode::from(1);
        }
        ExitCode::SUCCESS
    } else {
        run_buffered(path, bluecsv::unalign)
    }
}

fn run_buffered(path: &str, transform: fn(&str) -> String) -> ExitCode {
    let input = match read_input(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("bluecsv: {path}: {e}");
            return ExitCode::from(1);
        }
    };
    write_stdout(transform(&input).as_bytes())
}

fn run_infer(path: &str) -> ExitCode {
    let input = match read_input(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("bluecsv: {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let rows = bluecsv::parse(&input);
    let cols = bluecsv::infer_table(&rows, true);
    let mut out = String::new();
    out.push_str("col\ttype\tconfidence\tempty\tmismatches\n");
    for (i, c) in cols.iter().enumerate() {
        out.push_str(&format!(
            "{}\t{}\t{:.2}\t{}\t{}\n",
            i,
            c.primary.label(),
            c.confidence,
            c.empty_count,
            c.mismatch_rows.len()
        ));
    }
    write_stdout(out.as_bytes())
}

fn run_stats(path: &str, col: &str) -> ExitCode {
    let input = match read_input(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("bluecsv: {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let col_idx: usize = match col.parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("bluecsv: invalid column index: {col}");
            return ExitCode::from(2);
        }
    };
    let rows = bluecsv::parse(&input);
    let cols = bluecsv::infer_table(&rows, true);
    let Some(col_ty) = cols.get(col_idx) else {
        eprintln!("bluecsv: column {col_idx} out of range");
        return ExitCode::from(1);
    };
    let values = rows
        .iter()
        .skip(1)
        .filter_map(|r| r.get(col_idx).map(String::as_str));
    let s = bluecsv::summarize(values, col_ty.primary);

    let mut out = String::new();
    out.push_str(&format!("type\t{}\n", s.ty.label()));
    out.push_str(&format!("count\t{}\n", s.count));
    out.push_str(&format!("empty\t{}\n", s.empty));
    out.push_str(&format!("distinct\t{}\n", s.distinct));
    if let Some(v) = &s.min {
        out.push_str(&format!("min\t{v}\n"));
    }
    if let Some(v) = &s.max {
        out.push_str(&format!("max\t{v}\n"));
    }
    if let Some(v) = s.sum {
        out.push_str(&format!("sum\t{v}\n"));
    }
    if let Some(v) = s.mean {
        out.push_str(&format!("mean\t{v}\n"));
    }
    write_stdout(out.as_bytes())
}

fn write_stdout(bytes: &[u8]) -> ExitCode {
    let mut stdout = io::stdout().lock();
    if let Err(e) = stdout.write_all(bytes) {
        eprintln!("bluecsv: write: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn read_input(path: &str) -> io::Result<String> {
    if path == "-" {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        fs::read_to_string(path)
    }
}
