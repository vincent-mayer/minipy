# minipy

[![CI](https://github.com/vincent-mayer/minipy/actions/workflows/ci.yml/badge.svg)](https://github.com/vincent-mayer/minipy/actions/workflows/ci.yml)

A Python interpreter for a small subset of Python 3, written from scratch in Rust with
**zero dependencies** — hand-written lexer (including `INDENT`/`DEDENT`), precedence-climbing parser,
and tree-walking evaluator.

Every minipy program is valid Python 3, which makes CPython the test oracle: the examples
in [`examples/`](examples/) are verified byte-for-byte against `python3` in CI.

```console
$ cat examples/fib.py
def fib(n):
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)

for i in range(10):
    print(i, fib(i))

$ minipy examples/fib.py
0 0
1 1
2 1
...

$ diff <(python3 examples/fib.py) <(minipy examples/fib.py) && echo "identical"
identical
```

## Install

```sh
git clone https://github.com/vincent-mayer/minipy
cd minipy
cargo build --release
./target/release/minipy examples/fib.py   # run a script
./target/release/minipy                   # interactive REPL
```

## The subset

| | |
|---|---|
| **Types** | `int` (i64), `float`, `bool`, `str`, `None`, functions, `range` |
| **Statements** | assignment (`=`, `+=`, `-=`, `*=`, `/=`), `if`/`elif`/`else`, `while`, `for … in`, `def`, `return`, `pass`, `break`, `continue` |
| **Operators** | `or` `and` `not`, `==` `!=` `<` `<=` `>` `>=` (chained), `+` `-` `*` `/` `//` `%` `**`, unary `-` `+` |
| **Builtins** | `print` `len` `range` `str` `int` `float` `bool` `abs` `min` `max` |
| **Also** | closures, recursion, comments, `'…'`/`"…"` strings with escapes |

`bool` is a subtype of `int`, `/` always yields a float, and `//`/`%` floor toward −∞ and
take the divisor's sign — as in CPython, not as in Rust.

## Not supported

Lists, tuples, dicts, sets, slicing, classes, exceptions, `import`, `lambda`,
comprehensions, generators, decorators, default and keyword arguments, tuple unpacking,
`with`, `del`, `is`, `in`, f-strings, and triple-quoted strings.

## Known differences from CPython

- **Assignment always binds in the innermost scope.** There is no `global`/`nonlocal`, so a
  function cannot rebind a global — it shadows it instead. (CPython rejects the same code
  without `global`, so no correct Python program changes meaning.)
- **Integers are 64-bit**, not arbitrary precision; overflow raises `OverflowError`.
- **Recursion is capped at 1000 frames**, then `RecursionError`.
- The REPL reads plain lines: no history, no arrow keys. A block ends at a blank line.

## Layout

| File | |
|---|---|
| [`src/lexer.rs`](src/lexer.rs) | source → tokens, indentation stack, implicit line joining |
| [`src/parser.rs`](src/parser.rs) | tokens → AST; recursive descent for statements, precedence climbing for expressions |
| [`src/interp.rs`](src/interp.rs) | AST → execution; scopes, control flow, calls |
| [`src/value.rs`](src/value.rs) | the value model and Python's arithmetic and comparison rules |
| [`tests/`](tests/) | golden-file suites: `cases/*.py` + `.out`, `errors/*.py` + `.err` |

## License

MIT
