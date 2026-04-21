use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: bluecsv <align|unalign> <path|->");
        return ExitCode::from(2);
    }

    let input = match read_input(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("bluecsv: {}: {}", args[2], e);
            return ExitCode::from(1);
        }
    };

    let output = match args[1].as_str() {
        "align" => bluecsv::align(&input),
        "unalign" => bluecsv::unalign(&input),
        other => {
            eprintln!("bluecsv: unknown command: {other}");
            return ExitCode::from(2);
        }
    };

    let mut stdout = io::stdout().lock();
    if let Err(e) = stdout.write_all(output.as_bytes()) {
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
