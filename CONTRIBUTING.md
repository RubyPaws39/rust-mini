# Contributing To Rust Mini

Thanks for helping Rust Mini grow.

Rust Mini is a Rust-like scripting language. Contributions should keep the language close to Rust syntax and semantics where practical.

## Project Goals

- preserve real Rust-style syntax
- keep implementation readable for learning
- improve safety checks over time
- keep examples beginner-friendly
- add tests for language behavior
- avoid fake shortcut syntax that does not look like Rust

## Good First Issues

- add examples in `examples/`
- improve README or tutorial sections
- add parser/typechecker tests
- add missing string methods
- add more `Option` and `Result` methods
- improve diagnostics
- improve module/import behavior
- improve chess prototype rules
- improve terminal game helpers

## Development Setup

Install Rust, then:

```powershell
git clone <repo-url>
cd rust_mini
cargo test
cargo run -- examples\hello.rmini
```

## Before Opening A Pull Request

Run:

```powershell
cargo fmt
cargo test
```

For language features, add or update:

- parser support
- AST if needed
- type checker behavior
- borrow checker behavior if ownership is involved
- interpreter behavior
- tests in `tests/language.rs`
- an example in `examples/` when useful
- README/tutorial notes

## Language Design Rules

Do:

- prefer Rust-like syntax
- keep errors readable
- keep changes small
- add tests
- explain behavior in docs

Do not:

- add fake syntax like `set x = 5`
- bypass type checking for convenience
- weaken borrow checking without a reason
- add large unrelated refactors with a feature

## Pull Request Style

Use a short title:

```text
Add range for loops
```

In the description, include:

- what changed
- why it changed
- examples
- test command used

## Code Layout

```text
src/lexer.rs       tokenizes source
src/parser.rs      builds AST
src/ast.rs         syntax tree
src/typecheck.rs   checks types
src/borrowcheck.rs checks moves and borrows
src/interpreter.rs executes programs
src/value.rs       runtime values
src/error.rs       diagnostics
```

## Reporting Bugs

Please include:

- Rust Mini source code that fails
- command used
- expected behavior
- actual error/output
- OS and Rust version if relevant

Minimal examples help most.
