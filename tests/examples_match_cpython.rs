//! The examples are valid Python 3, so CPython is the oracle: run each one
//! under `python3` and under minipy, and require identical output.
//!
//! Skipped, with a note, where `python3` is unavailable.

use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn examples_print_exactly_what_cpython_prints() {
    let python = match Command::new("python3").arg("--version").output() {
        Ok(output) if output.status.success() => "python3",
        _ => {
            eprintln!("skipping: python3 is not available to compare against");
            return;
        }
    };

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut examples: Vec<_> = fs::read_dir(&dir)
        .expect("could not read examples/")
        .map(|entry| entry.expect("could not read a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "py"))
        .collect();
    examples.sort();
    assert!(!examples.is_empty(), "no examples to check");

    let mut failures = Vec::new();
    for example in examples {
        let name = example.file_name().unwrap().to_string_lossy().to_string();
        let reference = Command::new(python)
            .arg(&example)
            .output()
            .expect("could not run python3");
        assert!(
            reference.status.success(),
            "{name} is not valid Python: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let expected = String::from_utf8_lossy(&reference.stdout).into_owned();

        let source = fs::read_to_string(&example).expect("could not read the example");
        let actual = minipy::on_interpreter_stack(move || minipy::capture(&source));

        match actual {
            Ok(actual) if actual == expected => {}
            Ok(actual) => failures.push(format!(
                "{name}:\n--- cpython ---\n{expected}--- minipy ---\n{actual}"
            )),
            Err(err) => failures.push(format!("{name}: minipy failed: {err}")),
        }
    }

    assert!(
        failures.is_empty(),
        "\n\n{}\n{} example(s) differ from CPython\n",
        failures.join("\n"),
        failures.len()
    );
}
