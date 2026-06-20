# Rust Mini Benchmarks

These are small local benchmarks for Rust Mini compared with scripting runtimes in the same rough tier.

They are not scientific language shootout numbers. They are useful project statistics for tracking Rust Mini over time.

## Environment

- OS: Windows
- Rust Mini: `v1.0.0-beta.1`
- Rust Mini mode: release binary, full CLI pipeline
- Rust Mini VM: bytecode MVP via `--vm`
- Rust Mini AST: original AST interpreter via `--ast-run`
- Python: Python 3.11.8
- JavaScript: Node.js 24.14.1
- Native Rust: `rustc 1.95.0 -O`
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
| `sum_loop` | Rust Mini VM | `19999900000` | 102.31 | 109.38 | 141.53 |
| `sum_loop` | Rust Mini AST | `19999900000` | 208.65 | 214.35 | 220.82 |
| `sum_loop` | Python 3.11.8 | `19999900000` | 50.92 | 53.21 | 54.78 |
| `sum_loop` | Node.js 24.14.1 | `19999900000` | 192.55 | 198.41 | 204.38 |
| `sum_loop` | Native Rust | `19999900000` | 71.13 | 75.19 | 79.99 |
| `fib24` | Rust Mini VM | `46368` | 127.80 | 137.29 | 151.77 |
| `fib24` | Rust Mini AST | `46368` | 165.08 | 172.68 | 185.88 |
| `fib24` | Python 3.11.8 | `46368` | 39.88 | 40.72 | 42.02 |
| `fib24` | Node.js 24.14.1 | `46368` | 190.84 | 198.01 | 211.71 |
| `fib24` | Native Rust | `46368` | 72.31 | 76.05 | 82.60 |

## VM Speedup

| Benchmark | VM vs AST | VM vs Node.js | VM vs Python | VM vs Native Rust |
|---|---:|---:|---:|---:|
| `sum_loop` | 1.96x faster | 1.81x faster | 2.06x slower | 1.45x slower |
| `fib24` | 1.26x faster | 1.44x faster | 3.37x slower | 1.81x slower |

## Takeaways

Rust Mini now has a bytecode MVP. On these tests, VM mode is faster than the original AST interpreter and faster than Node.js for these tiny command-line runs.

Python is faster on these two tests because CPython has mature bytecode execution and highly optimized startup/runtime paths.

Native Rust is faster than Rust Mini, as expected, but these measurements include process startup and printing, so the native Rust advantage is partly hidden by command launch overhead.

Rust Mini still falls back to the AST interpreter for unsupported features. Future speed work:

- cache parsed modules
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
target\release\rmini.exe --vm benchmarks\sum_loop.rmini
target\release\rmini.exe --vm benchmarks\fib.rmini
```

