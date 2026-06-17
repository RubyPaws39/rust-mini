# Rust Mini

Rust Mini is a Rust-like interpreted scripting language written in Rust.

Goal: keep the nature of Rust while staying small enough to study. Rust Mini uses a real Rust-style subset: `fn main`, `let mut`, structs, enums, methods, traits, references, moves, borrows, modules, vectors, arrays, and a small standard library.

Current version: `1.0.0-beta.1`

Status: learning language, serious MVP, not full Rust.

## Quick Start

```powershell
cd R:\Rust\rust_mini
cargo run -- examples\hello.rmini
```

Downloaded the ZIP? Read [INSTALL_AND_USE.md](INSTALL_AND_USE.md).

Check a program without running it:

```powershell
cargo run -- --check examples\hello.rmini
```

Inspect tokens or AST:

```powershell
cargo run -- --tokens examples\hello.rmini
cargo run -- --ast examples\hello.rmini
```

## Small Example

```rust
fn main() {
    let mut hp: i64 = 10;
    damage(&mut hp);
    println!("hp {}", hp);
}

fn damage(value: &mut i64) {
    *value = *value - 1;
}
```

## What Rust Mini Can Do

- run `.rmini` scripts from the command line
- lex, parse, type-check, borrow-check, then interpret programs
- use Rust-like syntax instead of fake simplified syntax
- catch basic type errors
- catch basic ownership and borrow errors
- run demos like calculator, RPG, Pong, and chess prototype

## Language Features

Supported core syntax:

- `fn name(...) { ... }`
- `let` and `let mut`
- `i64`, `f64`, `bool`, `String`, unit `()`
- arrays: `[i64; 3]`, `[1, 2, 3]`
- vectors: `Vec<i64>`, `vec![1, 2, 3]`
- tuples: `(i64, bool)`, `(1, true)`
- blocks and tail expressions
- `if`, `else`, `else if`
- `while`
- `loop`
- `for item in array`
- `for item in vec`
- `for ch in text`
- ranges: `for i in 0..10`
- `break`, `continue`
- `return`
- assignment
- index assignment: `xs[0] = 10`
- field assignment: `player.hp = 9`
- function calls
- method calls
- formatting macros: `format!`, `print!`, `println!`

Data modeling:

- structs
- enums
- built-in `Option<T>`
- built-in `Result<T, E>`
- basic `match` over enum variants
- basic traits and `impl Trait for Type`
- inherent `impl` methods
- `self`, `&self`, `&mut self`

References and safety:

- immutable references: `&x`
- mutable references: `&mut x`
- dereference: `*x`
- move checking
- simple borrow checking
- no garbage collector

## Standard Library

Rust Mini has a small host standard library:

```rust
print(value)
len(value)
push(&mut vec, value)
args()
env(name)
clock_ms()
concat(left, right)
contains(text, needle)
input(prompt)
read_key()
parse_i64(text)
parse_f64(text)
unwrap_or(value, default)
clear()
sleep_ms(ms)
rand_i64(min, max)
color(text, color)
read_file(path)
write_file(path, text)
```

Rust-style paths:

```rust
std::io::read_line("name: ");
std::io::read_i64("number: ", 0);
std::fs::read_to_string("file.txt");
std::fs::write("file.txt", "hello");
std::time::sleep_ms(100);
game::clear();
game::read_key();
```

Imported short paths:

```rust
use std::io;
use std::fs;
use std::time;

fn main() {
    let n: i64 = io::read_i64("number: ", 0);
    fs::write("out.txt", "saved");
    time::sleep_ms(100);
    println!("number {}", n);
}
```

Common methods:

```rust
xs.len();
xs.push(4);
xs.pop();

text.trim();
text.push_str("!");
text.to_lowercase();
text.to_uppercase();
text.replace("old", "new");
text.contains("needle");
text.starts_with("prefix");
text.ends_with("suffix");

value.is_some();
value.is_none();
value.unwrap_or(0);

result.is_ok();
result.is_err();
result.unwrap_or(0);
```

## Borrow Rules

Rust Mini tracks simplified ownership and borrowing:

- `i64`, `f64`, `bool`, unit, and references are Copy
- `String`, structs, enums, arrays, tuples, and vecs move by default
- moved values cannot be used
- many immutable borrows are allowed
- only one mutable borrow is allowed
- mutable and immutable borrows cannot overlap
- variables cannot be assigned while borrowed
- `&mut` can only borrow mutable variables
- functions cannot return references
- compound values cannot store references yet

Example move error:

```rust
fn main() {
    let name: String = "Ruby";
    let other = name;
    print(name);
}
```

Expected idea:

```text
borrowcheck error: use of moved value `name`
```

## Example Programs

Run any of these:

```powershell
cargo run -- examples\hello.rmini
cargo run -- examples\math.rmini
cargo run -- examples\functions.rmini
cargo run -- examples\borrow_ok.rmini
cargo run -- examples\control_flow.rmini
cargo run -- examples\data_types.rmini
cargo run -- examples\vecs.rmini
cargo run -- examples\traits_demo.rmini
cargo run -- examples\format_and_string_loop.rmini
cargo run -- examples\std_io_and_methods.rmini
cargo run -- examples\range_and_result_methods.rmini
cargo run -- examples\string_methods.rmini
cargo run -- examples\interactive_calculator.rmini
cargo run -- examples\rpg_demo.rmini
cargo run -- examples\animated_pong.rmini
cargo run -- examples\chess_prototype.rmini
```

Pass script arguments after `--`:

```powershell
cargo run -- examples\host_std.rmini -- first second
```

## Project Layout

```text
src/
  main.rs          CLI
  lexer.rs         tokenizer
  token.rs         token definitions
  parser.rs        recursive descent / Pratt parser
  ast.rs           syntax tree
  typecheck.rs     type checker
  borrowcheck.rs   simplified borrow checker
  interpreter.rs   runtime/interpreter
  value.rs         runtime values
  error.rs         diagnostics

lib/
  std.rmini        small standard library wrappers
  game.rmini       terminal game helpers

examples/
  *.rmini          example programs

tests/
  language.rs      integration tests
```

## Development

Run tests:

```powershell
cargo test
```

Format Rust code:

```powershell
cargo fmt
```

Good first contribution areas:

- improve error messages
- add examples
- add tests for existing syntax
- expand string methods
- add more `Option` and `Result` methods
- improve module/import behavior
- improve chess prototype rules
- improve terminal game helpers

## Known Limits

Rust Mini is not full Rust yet.

Missing or incomplete:

- user-defined generics
- trait bounds
- trait objects
- full lifetime syntax
- non-lexical lifetimes
- closures
- full macro system
- `?` operator
- full pattern matching
- destructuring `let`
- real crate/package manager
- full module privacy
- threads and async
- large standard library
- parser recovery
- full chess/FIDE engine
- GUI/window backend

`read_key()` currently has Windows terminal support first. Other platforms return no key until platform input is added.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

Best way to help:

1. Open an issue with the feature or bug.
2. Keep changes small.
3. Add tests.
4. Keep syntax Rust-like.
5. Run `cargo test` before opening a pull request.

## License

License not chosen yet. Pick one before publishing broadly, usually MIT or Apache-2.0 for open source Rust projects.
