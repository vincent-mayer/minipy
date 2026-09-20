//! The `minipy` command: run a script, or start a REPL when given no script.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use minipy::{Repl, is_incomplete, run_source};

const USAGE: &str = "\
usage: minipy [script.py]

Run a minipy script, or start an interactive session when no script is given.

options:
  -h, --help     show this message and exit
  -V, --version  show the version and exit";

fn main() -> ExitCode {
    let mut script: Option<String> = None;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("minipy {}", minipy::VERSION);
                return ExitCode::SUCCESS;
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                eprintln!("minipy: unknown option '{arg}'\n\n{USAGE}");
                return ExitCode::from(2);
            }
            _ if script.is_some() => {
                eprintln!("minipy: expected at most one script\n\n{USAGE}");
                return ExitCode::from(2);
            }
            _ => script = Some(arg),
        }
    }

    // The interpreter needs a deeper stack than a thread gets by default.
    minipy::on_interpreter_stack(move || match script {
        Some(path) => run_file(&path),
        None => repl(),
    })
}

fn run_file(path: &str) -> ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(err) => {
            eprintln!("minipy: can't open file '{path}': {err}");
            return ExitCode::from(2);
        }
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let result = run_source(&src, &mut out);
    let _ = out.flush();

    match result {
        Ok(()) => ExitCode::SUCCESS,
        // `minipy script.py | head` closes the pipe early; that is not a
        // failure of the program, so say nothing and exit cleanly.
        Err(err) if err.is_broken_pipe() => ExitCode::SUCCESS,
        Err(err) if err.kind == minipy::ErrorKind::Io => {
            eprintln!("minipy: {}", err.msg);
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("{}", err.report(path));
            ExitCode::FAILURE
        }
    }
}

fn repl() -> ExitCode {
    println!("minipy {} — Ctrl-D to exit", minipy::VERSION);

    let mut session = Repl::new();
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut buffer = String::new();

    loop {
        let prompt = if buffer.is_empty() { ">>> " } else { "... " };
        let _ = write!(out, "{prompt}");
        let _ = out.flush();

        let Some(line) = lines.next() else { break };
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                let _ = writeln!(out, "minipy: {err}");
                break;
            }
        };

        // A continuation ends at a blank line; a fresh blank line is a no-op.
        if buffer.is_empty() && line.trim().is_empty() {
            continue;
        }
        buffer.push_str(&line);
        buffer.push('\n');
        // A multi-line block keeps collecting until the user enters a blank line.
        let multiline = buffer.trim_end().contains('\n');
        if is_incomplete(&buffer) || (multiline && !line.trim().is_empty()) {
            continue;
        }

        if let Err(err) = session.feed(&buffer, &mut out) {
            if err.is_broken_pipe() {
                break;
            }
            let _ = out.flush();
            eprintln!("{}", err.report("<stdin>"));
        }
        let _ = out.flush();
        buffer.clear();
    }

    let _ = writeln!(out);
    ExitCode::SUCCESS
}
