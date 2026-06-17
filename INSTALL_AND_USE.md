# Install And Use Rust Mini From ZIP

This guide is for someone who downloaded Rust Mini as a ZIP file from GitHub.

## 1. Install Rust

Rust Mini is written in Rust, so you need Rust installed first.

Download Rust:

```text
https://www.rust-lang.org/tools/install
```

On Windows, install Rust with `rustup-init.exe`.

After installing, open a new PowerShell window and check:

```powershell
rustc --version
cargo --version
```

If both commands print versions, Rust is ready.

## 2. Download Rust Mini

On GitHub:

1. Open the Rust Mini repository.
2. Click **Code**.
3. Click **Download ZIP**.
4. Extract the ZIP file.

Example extracted folder:

```text
C:\Users\YourName\Downloads\rust-mini-main
```

## 3. Open A Terminal In The Project

In PowerShell:

```powershell
cd C:\Users\YourName\Downloads\rust-mini-main
```

Use the folder where you extracted the ZIP.

## 4. Run Hello World

```powershell
cargo run -- examples\hello.rmini
```

Expected output:

```text
Hello from Rust Mini!
```

First run may compile slowly. Later runs are faster.

## 5. Run Other Examples

```powershell
cargo run -- examples\math.rmini
cargo run -- examples\functions.rmini
cargo run -- examples\borrow_ok.rmini
cargo run -- examples\interactive_calculator.rmini
cargo run -- examples\rpg_demo.rmini
cargo run -- examples\animated_pong.rmini
```

Animated Pong controls:

```text
w / s
arrow up / arrow down
q quit
```

## 6. Check A Program Without Running

```powershell
cargo run -- --check examples\hello.rmini
```

Expected output:

```text
check ok
```

## 7. See Tokens Or AST

Print tokens:

```powershell
cargo run -- --tokens examples\hello.rmini
```

Print AST:

```powershell
cargo run -- --ast examples\hello.rmini
```

## 8. Write Your Own Script

Create a file:

```text
examples\my_program.rmini
```

Example code:

```rust
fn main() {
    let mut hp: i64 = 10;
    hp = hp - 1;
    println!("hp {}", hp);
}
```

Run it:

```powershell
cargo run -- examples\my_program.rmini
```

## 9. Use Input

Example:

```rust
use std::io;

fn main() {
    let name: String = io::read_line("name: ");
    println!("hello {}", name);
}
```

Run:

```powershell
cargo run -- examples\my_program.rmini
```

## 10. Common Problems

If PowerShell says `cargo` is not found:

- install Rust
- close PowerShell
- open PowerShell again
- run `cargo --version`

If Rust Mini says it cannot read a file:

- make sure the path points to a `.rmini` file
- do not pass a folder name by mistake

Correct:

```powershell
cargo run -- examples\hello.rmini
```

Wrong:

```powershell
cargo run -- examples examples\hello.rmini
```

## 11. Learn The Language

Read:

```text
README.md
RUST_MINI_TUTORIAL.md
```

Good beginner examples:

```text
examples\hello.rmini
examples\math.rmini
examples\functions.rmini
examples\data_types.rmini
examples\vecs.rmini
examples\range_and_result_methods.rmini
```
