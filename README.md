# the-evil-compiler

A compiler for the **HULK** language, written in Rust. It takes a `.hulk` source
file, runs the full front-end (lexer → parser → generics → type inference →
semantic checks), emits **LLVM IR** (`output.ll`), and links it into a native
Linux x86-64 executable (`./output`).

For a deep dive into the architecture and design decisions, see [`REPORT.md`](REPORT.md).

## Project layout

```
src/
  main.rs            # entry point / compilation pipeline
  lexer/             # hand-written lexer + tokens
  grammar.lalrpop     # LALR(1) grammar
  nodes/             # AST nodes
  generics/          # promotion + monomorphization
  type_inferrer.rs   # constraint-based type inference
  semantic.rs        # semantic checks
  codegen*.rs        # LLVM IR generation
  errors.rs          # diagnostics + exit codes
```

## Requirements

- **Rust** (stable, with Cargo)
- **LLVM toolchain** with `clang` available on `PATH` (falls back to `llc` + `cc`)

## Build

```bash
cd compiler
cargo build --release
```

## Run

Pass a `.hulk` file as the only argument:

```bash
cargo run -- path/to/program.hulk
# or, after building:
./target/release/hulk path/to/program.hulk
```

On success this writes `output.ll` and produces the executable `./output`, which
you can then run:

```bash
./output
```

### Exit codes

The process exit code tells you which phase (if any) failed:

| Code | Meaning   |
|------|-----------|
| 0    | Success   |
| 1    | Lexical error   |
| 2    | Syntactic error |
| 3    | Semantic error  |

Diagnostics are printed to `stderr` in the format `(line,col) TYPE: message`.

## Tests

Tests are plain `.hulk` programs. They are in `compiler/tests` .The simplest workflow is to compile each one
and check that it builds and runs as expected.

### Run all test programs

Put your cases under `tests/` and run them in a loop:

```bash
for f in tests/*.hulk; do
  echo "=== $f ==="
  cargo run --quiet -- "$f" && ./output
  echo "exit code: $?"
done
```

If you also have Rust unit/integration tests (`#[test]` functions or files under
`tests/`), run them with:

```bash
cargo test
```

### Create a new test

1. Add a new program, e.g. `tests/my_case.hulk`:

   ```hulk
   function square(x: Number): Number => x * x;
   print(square(7));
   ```

2. Compile and run it:

   ```bash
   cargo run -- tests/my_case.hulk
   ./output
   ```

3. Check the result:
   - **Should compile:** exit code `0` and the expected program output.
   - **Should fail:** the expected exit code (`1`/`2`/`3`) and the diagnostic
     message you want.

A handy convention is to keep an expected-output file next to each case (e.g.
`my_case.expected`) and diff it against the program's output to make failures
easy to spot.
