# Rust Mini Benchmarks

These are small local benchmarks for Rust Mini compared with scripting runtimes in the same rough tier.

They are not scientific language shootout numbers. They are useful project statistics for tracking Rust Mini over time.

## Environment

- OS: Windows
- Rust Mini: `v1.0.0-beta.1`
- Rust Mini mode: release binary, full CLI pipeline
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
| `sum_loop` | Rust Mini | `19999900000` | 294.18 | 317.62 | 344.07 |
| `sum_loop` | Python 3.11 | `19999900000` | 93.64 | 105.07 | 126.84 |
| `sum_loop` | Node.js | `19999900000` | 259.81 | 272.85 | 282.21 |
| `sum_loop` | Native Rust | `19999900000` | 122.07 | 140.86 | 173.06 |
| `fib24` | Rust Mini | `46368` | 244.22 | 256.49 | 270.30 |
| `fib24` | Python 3.11 | `46368` | 81.75 | 97.36 | 115.29 |
| `fib24` | Node.js | `46368` | 259.29 | 281.41 | 319.59 |
| `fib24` | Native Rust | `46368` | 112.33 | 123.98 | 135.88 |

## Takeaways

Rust Mini is already in the same broad startup/runtime band as Node.js for these tiny command-line scripts.

Python is faster on these two tests because CPython has mature bytecode execution and highly optimized startup/runtime paths.

Native Rust is faster than Rust Mini, as expected, but these measurements include process startup and printing, so the native Rust advantage is partly hidden by command launch overhead.

Rust Mini currently interprets AST directly. Future speed work:

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
