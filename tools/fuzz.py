#!/usr/bin/env python3
"""Differential fuzzing against CPython.

Generates random programs in the minipy subset, runs each under both CPython
and minipy, and reports any difference in output or in exception type. Because
every minipy program is valid Python 3, CPython is the oracle.

    cargo build
    python3 tools/fuzz.py                 # both modes, seeds 0-4
    python3 tools/fuzz.py --mode exprs --seeds 20
    python3 tools/fuzz.py --show          # print the first differing program

Exits non-zero if anything differs.
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

MINIPY = os.environ.get("MINIPY", "./target/debug/minipy")

ATOMS = ["0", "1", "2", "3", "7", "-1", "10", "2.5", "0.5", "-3.5", "True", "False",
         "'a'", "'ab'", "''", "None", "1e3"]
ARITH = ["+", "-", "*", "/", "//", "%", "**"]
COMPARE = ["==", "!=", "<", "<=", ">", ">="]
BOOLEAN = ["and", "or"]
NAMES = ["a", "b", "c", "n"]


def expression(rng, depth=0, names=()):
    r = rng.random()
    if depth > 2 or r < 0.35:
        return rng.choice(list(names) + ATOMS)
    if r < 0.60:
        return f"({expression(rng, depth+1, names)} {rng.choice(ARITH)} {expression(rng, depth+1, names)})"
    if r < 0.75:
        return f"({expression(rng, depth+1, names)} {rng.choice(COMPARE)} {expression(rng, depth+1, names)})"
    if r < 0.85:
        return f"({expression(rng, depth+1, names)} {rng.choice(BOOLEAN)} {expression(rng, depth+1, names)})"
    if r < 0.92:
        return f"(not {expression(rng, depth+1, names)})"
    if r < 0.96:
        return f"(-{expression(rng, depth+1, names)})"
    return (f"({expression(rng, depth+1, names)} {rng.choice(COMPARE)} "
            f"{expression(rng, depth+1, names)} {rng.choice(COMPARE)} {expression(rng, depth+1, names)})")


def expression_program(rng):
    return f"print({expression(rng)})\n"


def statements(rng, indent, depth):
    pad = " " * indent
    r = rng.random()
    if depth > 2 or r < 0.40:
        return [f"{pad}{rng.choice(NAMES)} {rng.choice(['=', '+=', '-=', '*='])} {expression(rng, names=NAMES)}"]
    if r < 0.55:
        return [f"{pad}print({expression(rng, names=NAMES)})"]
    if r < 0.70:
        out = [f"{pad}if {expression(rng, names=NAMES)}:"] + block(rng, indent + 4, depth + 1)
        if rng.random() < 0.5:
            out += [f"{pad}else:"] + block(rng, indent + 4, depth + 1)
        return out
    if r < 0.85:
        return ([f"{pad}for {rng.choice(NAMES)} in range({rng.randint(0, 4)}):"]
                + block(rng, indent + 4, depth + 1))
    name = f"f{rng.randint(0, 3)}"
    body = block(rng, indent + 4, depth + 1) + [f"{pad}    return {expression(rng, names=NAMES)}"]
    return ([f"{pad}def {name}({rng.choice(NAMES)}):"] + body
            + [f"{pad}print({name}({expression(rng, names=NAMES)}))"])


def block(rng, indent, depth):
    lines = []
    for _ in range(rng.randint(1, 3)):
        lines += statements(rng, indent, depth)
    return lines or [" " * indent + "pass"]


def statement_program(rng):
    lines = [f"{name} = {i}" for i, name in enumerate(NAMES)]
    for _ in range(rng.randint(2, 5)):
        lines += statements(rng, 0, 0)
    lines.append(f"print({', '.join(NAMES)})")
    return "\n".join(lines) + "\n"


def run(command, path):
    """Return ('ok', stdout) or ('err', ExceptionName), or None on timeout."""
    try:
        result = subprocess.run([*command, path], capture_output=True, text=True, timeout=15)
    except subprocess.TimeoutExpired:
        return None
    if result.returncode == 0:
        return ("ok", result.stdout)
    lines = [l for l in result.stderr.strip().splitlines() if l and not l.startswith(" ")]
    return ("err", lines[-1].split(":")[0] if lines else "?")


def compare(source):
    with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False) as handle:
        handle.write(source)
        path = handle.name
    try:
        reference, actual = run(["python3"], path), run([MINIPY], path)
        if reference is None or actual is None:
            return None
        return None if reference == actual else (reference, actual)
    finally:
        os.unlink(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mode", choices=["exprs", "programs", "both"], default="both")
    parser.add_argument("--seeds", type=int, default=5)
    parser.add_argument("--count", type=int, default=250, help="programs per seed")
    parser.add_argument("--show", action="store_true", help="print differing programs")
    args = parser.parse_args()

    if not os.path.exists(MINIPY):
        sys.exit(f"{MINIPY} not found — run `cargo build` first")

    modes = ["exprs", "programs"] if args.mode == "both" else [args.mode]
    generators = {"exprs": expression_program, "programs": statement_program}

    total = differences = 0
    for mode in modes:
        for seed in range(args.seeds):
            rng = random.Random(seed)
            for _ in range(args.count):
                source = generators[mode](rng)
                verdict = compare(source)
                total += 1
                if verdict is not None:
                    differences += 1
                    print(f"\n=== {mode} seed {seed} ===")
                    if args.show:
                        print(source, end="")
                    print(f"  cpython: {verdict[0]}\n  minipy : {verdict[1]}")

    print(f"\n{total} programs compared, {differences} difference(s)")
    return 1 if differences else 0


if __name__ == "__main__":
    sys.exit(main())
