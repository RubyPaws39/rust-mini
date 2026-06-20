# Rust Mini Benchmarks

These are small local benchmarks for Rust Mini compared with scripting runtimes in the same rough tier.

They are not scientific language shootout numbers. They are useful project statistics for tracking Rust Mini over time.

## Environment

- OS: Windows
- Rust Mini: `v1.0.0-beta.1`
- Rust Mini mode: release binary, full CLI pipeline
- Rust Mini VM: bytecode MVP via `--vm`
- Rust Mini AST: original AST interpreter via `--ast-run`
- Python: Python 3.11
- JavaScript: Node.js
- Native Rust: `rustc -O`
- Runs: 7 timed runs after one warm-up run
- Timing tool: PowerShell `Measure-Command`

Rust Mini timings include:

- file read
- lexing
- parsing
- type checking
- borrow checking
- interpretation

## Results

Lower is faster.

| Benchmark | Language | Output | Min ms | Avg ms | Max ms |
|---|---:|---:|---:|---:|---:|
| `sum_loop` | Rust Mini VM | `19999900000` | 161.52 | 169.31 | 187.54 |
| `sum_loop` | Rust Mini AST | `19999900000` | 281.32 | 291.53 | 299.14 |
| `sum_loop` | Python 3.11 | `19999900000` | 93.64 | 105.07 | 126.84 |
| `sum_loop` | Node.js | `19999900000` | 259.81 | 272.85 | 282.21 |
| `sum_loop` | Native Rust | `19999900000` | 122.07 | 140.86 | 173.06 |
| `fib24` | Rust Mini VM | `46368` | 182.99 | 190.72 | 197.82 |
| `fib24` | Rust Mini AST | `46368` | 216.47 | 227.54 | 236.54 |
| `fib24` | Python 3.11 | `46368` | 81.75 | 97.36 | 115.29 |
| `fib24` | Node.js | `46368` | 259.29 | 281.41 | 319.59 |
| `fib24` | Native Rust | `46368` | 112.33 | 123.98 | 135.88 |

## Takeaways

Rust Mini now has a bytecode MVP. On these tests, VM mode is faster than the original AST interpreter.

Python is faster on these two tests because CPython has mature bytecode execution and highly optimized startup/runtime paths.

Native Rust is faster than Rust Mini, as expected, but these measurements include process startup and printing, so the native Rust advantage is partly hidden by command launch overhead.

Rust Mini still falls back to the AST interpreter for unsupported features. Future speed work:

- cache parsed modules
- add bytecode
- skip checker passes in release script mode when requested
- optimize variable lookup
- optimize function calls
- add benchmark command automation

## Run Locally

Build Rust Mini:

```powershell
cargo build --release --offline
```

Compile native Rust comparison programs:

```powershell
rustc -O benchmarks\sum_loop.rs -o benchmarks\sum_loop_rust.exe
rustc -O benchmarks\fib.rs -o benchmarks\fib_rust.exe
```

Run Rust Mini benchmarks:

```powershell
target\release\rust_mini.exe benchmarks\sum_loop.rmini
target\release\rust_mini.exe benchmarks\fib.rmini
```

