# Rust Mini vs Rust

Same idea: Rust-style syntax, types, ownership lessons, safe error handling.

Different goal: Rust Mini is interpreted and smaller.

## Rust Mini Script

```rust
fn main() {
    let value: Result<i64, String> = parse_i64(input("number: "));

    match value {
        Result::Ok(n) => {
            print(n + 10);
        },
        Result::Err(message) => {
            print(message);
        },
    };
}
```

Run:

```powershell
cargo run -- examples/result_demo.rmini
```

## Similar Rust Program

```rust
use std::io::{self, Write};

fn main() {
    print!("number: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    let value: Result<i64, String> = input
        .trim()
        .parse::<i64>()
        .map_err(|_| format!("cannot parse `{}` as i64", input.trim()));

    match value {
        Ok(n) => println!("{}", n + 10),
        Err(message) => println!("{}", message),
    }
}
```

## Main Difference

Rust Mini:

- smaller syntax
- interpreted
- no compile-to-native binary yet
- simple builtins like `input`, `print`, `parse_i64`
- good for learning and scripting

Rust:

- full systems language
- native binary
- huge ecosystem
- advanced generics, traits, lifetimes, macros, async
- production-grade performance and tooling

## Best Description

Rust Mini is a Rust-like safe scripting language.

Rust is a full production systems language.
