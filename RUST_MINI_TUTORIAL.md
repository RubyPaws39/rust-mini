# Rust Mini Tutorial

Rust Mini is a Rust-like scripting language written in Rust. It keeps real Rust-style syntax where possible, but supports a smaller learning subset.

Rust Mini files use `.rmini`.

## 1. Run A Program

From the project folder:

```powershell
cd R:\Rust\rust_mini
cargo run -- examples\hello.rmini
```

Useful CLI modes:

```powershell
cargo run -- --check examples\hello.rmini
cargo run -- --tokens examples\hello.rmini
cargo run -- --ast examples\hello.rmini
cargo run -- --version
```

Pass arguments into a script after `--`:

```powershell
cargo run -- examples\host_std.rmini -- first second
```

## 2. Hello World

```rust
fn main() {
    println!("Hello from Rust Mini!");
}
```

Every program starts at `main`.

## 3. Variables

```rust
fn main() {
    let x: i64 = 5;
    let mut hp: i64 = 10;

    hp = hp - 1;

    println!("x {}", x);
    println!("hp {}", hp);
}
```

Use `let` for immutable values.

Use `let mut` for values that can change.

## 4. Basic Types

```rust
fn main() {
    let age: i64 = 30;
    let speed: f64 = 3.5;
    let alive: bool = true;
    let name: String = "Ruby";
    let nothing: () = ();

    println!("{}", age);
    println!("{}", speed);
    println!("{}", alive);
    println!("{}", name);
    println!("{}", nothing);
}
```

Supported core scalar types:

```rust
i64
f64
bool
String
()
```

## 5. Functions

```rust
fn add(a: i64, b: i64) -> i64 {
    return a + b;
}

fn main() {
    let result: i64 = add(2, 3);
    println!("result {}", result);
}
```

Functions can return values with `return`.

## 6. If And Else

```rust
fn main() {
    let hp: i64 = 10;

    if hp > 0 {
        println!("alive");
    } else {
        println!("down");
    }
}
```

Conditions must be `bool`.

## 7. Loops

`while`:

```rust
fn main() {
    let mut x: i64 = 0;

    while x < 3 {
        println!("{}", x);
        x = x + 1;
    }
}
```

`loop`, `break`, and `continue`:

```rust
fn main() {
    let mut x: i64 = 0;

    loop {
        x = x + 1;

        if x == 2 {
            continue;
        }

        println!("{}", x);

        if x == 4 {
            break;
        }
    }
}
```

`for` over ranges:

```rust
fn main() {
    for i in 0..5 {
        println!("{}", i);
    }
}
```

`for` over strings:

```rust
fn main() {
    for ch in "rust" {
        println!("letter {}", ch);
    }
}
```

`for` over arrays and vectors:

```rust
fn main() {
    let xs: [i64; 3] = [1, 2, 3];

    for x in xs {
        println!("{}", x);
    }
}
```

## 8. Arrays

```rust
fn main() {
    let mut xs: [i64; 3] = [10, 20, 30];

    println!("{}", xs[1]);

    xs[1] = 99;

    println!("{}", xs[1]);
    println!("len {}", xs.len());
}
```

Arrays have fixed length.

## 9. Vectors

```rust
fn main() {
    let mut xs: Vec<i64> = vec![1, 2, 3];

    xs.push(4);
    xs[0] = 10;

    println!("len {}", xs.len());
    println!("first {}", xs[0]);
    println!("last {}", xs.pop().unwrap_or(0));
}
```

Vectors can grow with `push`.

`pop` returns `Option<T>`.

## 10. Strings

```rust
fn main() {
    let mut name: String = "Ruby";

    name.push_str(" Mini");

    println!("{}", name.trim());
    println!("len {}", name.len());
}
```

Useful string helpers:

```rust
text.len()
text.trim()
text.push_str("more")
text.to_lowercase()
text.to_uppercase()
text.replace("old", "new")
text.contains("needle")
text.starts_with("prefix")
text.ends_with("suffix")
contains(text, "needle")
concat("hello", " world")
```

## 11. Formatting

Rust Mini supports simple Rust-like formatting macros:

```rust
fn main() {
    let hp: i64 = 10;
    let name: String = "Ruby";

    println!("{} has {} hp", name, hp);

    let message: String = format!("{} hp", hp);
    print!("{}", message);
}
```

Supported placeholders are `{}`. Escaped braces work with `{{` and `}}`.

## 12. Tuples

```rust
fn main() {
    let mut pair: (i64, bool) = (42, true);

    println!("{}", pair.0);
    println!("{}", pair.1);

    pair.0 = 100;

    println!("{}", pair.0);
}
```

Tuple fields use numbers: `.0`, `.1`, `.2`.

## 13. Structs

```rust
struct Player {
    hp: i64,
    power: i64,
}

fn main() {
    let mut player: Player = Player { hp: 10, power: 3 };

    println!("{}", player.hp);

    player.hp = player.hp - 1;

    println!("{}", player.hp);
}
```

Structs group named fields.

## 14. Methods

```rust
struct Player {
    hp: i64,
}

impl Player {
    fn is_alive(&self) -> bool {
        return self.hp > 0;
    }

    fn damage(&mut self, amount: i64) {
        self.hp = self.hp - amount;
    }
}

fn main() {
    let mut p: Player = Player { hp: 10 };

    println!("{}", p.is_alive());

    p.damage(3);

    println!("{}", p.hp);
}
```

Use `&self` for read-only methods.

Use `&mut self` for methods that change the value.

## 15. Traits

```rust
pub trait Named {
    fn name(&self) -> String;
}

pub struct Player {
    name: String,
    hp: i64,
}

impl Named for Player {
    fn name(&self) -> String {
        return self.name;
    }
}

fn main() {
    let player: Player = Player { name: "Ruby", hp: 10 };
    println!("{}", player.name());
}
```

Traits define required methods.

Current trait support is concrete-only. Rust Mini does not have trait bounds or trait objects yet.

## 16. Enums And Match

```rust
enum Maybe {
    Some(i64),
    None,
}

fn unwrap_or(value: Maybe, default: i64) -> i64 {
    return match value {
        Maybe::Some(x) => x,
        Maybe::None => default,
    };
}

fn main() {
    let a: Maybe = Maybe::Some(42);
    let b: Maybe = Maybe::None;

    println!("{}", unwrap_or(a, 0));
    println!("{}", unwrap_or(b, 0));
}
```

`match` must cover every enum variant.

## 17. Option And Result

```rust
fn main() {
    let value: Result<i64, String> = parse_i64("42");

    println!("{}", value.is_ok());
    println!("{}", value.unwrap_or(0));

    let maybe: Option<i64> = Option::Some(7);

    println!("{}", maybe.is_some());
    println!("{}", maybe.unwrap_or(99));
}
```

`parse_i64`, `parse_f64`, and `read_file` return `Result`.

Use `match` for careful handling:

```rust
fn main() {
    let value: Result<i64, String> = parse_i64("oops");

    match value {
        Result::Ok(n) => println!("{}", n),
        Result::Err(message) => println!("{}", message),
    };
}
```

## 18. References

```rust
fn damage(hp: &mut i64) {
    *hp = *hp - 1;
}

fn main() {
    let mut hp: i64 = 10;

    damage(&mut hp);

    println!("{}", hp);
}
```

Use `&x` for immutable references.

Use `&mut x` for mutable references.

Use `*x` to dereference.

## 19. Borrow Rules

Rust Mini checks ownership and borrowing.

Rules:

- moved values cannot be used
- many immutable borrows allowed
- only one mutable borrow allowed
- mutable and immutable borrows cannot overlap
- cannot assign to a borrowed value
- `&mut` can only borrow mutable variables
- functions cannot return references
- structs, enums, tuples, arrays, and vecs cannot store references yet

Move error:

```rust
fn main() {
    let name: String = "Ruby";
    let other = name;

    println!("{}", name);
}
```

Borrow conflict:

```rust
fn main() {
    let mut x: i64 = 5;

    let a = &x;
    let b = &mut x;
}
```

## 20. Modules

Split code across files with `mod`.

```rust
mod game_math;
use game_math::*;

fn main() {
    let mut velocity: Vec2 = Vec2 { x: 2.0, y: 3.0 };
    velocity.scale(3.0);

    println!("{}", velocity.x);
    println!("{}", velocity.y);
}
```

`mod game_math;` loads `game_math.rmini` next to the current file. If that file does not exist, Rust Mini tries `game_math/mod.rmini`.

For now, imports are simpler than real Rust. Names are merged globally.

## 21. Standard And Game Libraries

Rust Mini searches `lib/` for imported modules.

```rust
use std::io;
use std::fs;
use std::time;

fn main() {
    let n: i64 = io::read_i64("number: ", 7);
    println!("next {}", n + 1);

    fs::write("examples/out.txt", "saved");
    time::sleep_ms(100);
}
```

Game helpers:

```rust
fn main() {
    game::clear();
    println!("{}", game::color("Hello in cyan", "cyan"));
    game::sleep_ms(500);
    println!("{}", game::rand_i64(1, 6));
}
```

`game::read_key()` polls keyboard input without waiting for Enter on Windows terminals.

## 22. File I/O

```rust
fn main() {
    let text: String = read_file("examples/message.txt").unwrap_or("");

    println!("{}", text);
    println!("len {}", text.len());

    write_file("examples/out.txt", "written by Rust Mini");
}
```

Paths are relative to the folder where the program runs.

## 23. User Input

```rust
use std::io;

fn main() {
    let left: f64 = io::read_f64("left number: ", 0.0);
    let right: f64 = io::read_f64("right number: ", 0.0);

    println!("sum {}", left + right);
}
```

`input(prompt)` reads one line from the terminal.

`parse_i64(text)` and `parse_f64(text)` return `Result`.

## 24. Bigger Example

```rust
enum Action {
    Attack(i64),
    Heal(i64),
    Wait,
}

struct Fighter {
    hp: i64,
    power: i64,
}

impl Fighter {
    fn score(&self) -> i64 {
        return self.hp + self.power;
    }

    fn apply(&mut self, action: Action) {
        self.hp = match action {
            Action::Attack(amount) => self.hp - amount,
            Action::Heal(amount) => self.hp + amount,
            Action::Wait => self.hp,
        };
    }
}

fn main() {
    let mut hero: Fighter = Fighter { hp: 20, power: 5 };

    hero.apply(Action::Attack(4));
    hero.apply(Action::Heal(2));

    println!("{}", hero.hp);
    println!("{}", hero.score());
}
```

## 25. Try The Demos

```powershell
cargo run -- examples\interactive_calculator.rmini
cargo run -- examples\rpg_demo.rmini
cargo run -- examples\animated_pong.rmini
cargo run -- examples\chess_prototype.rmini
cargo run -- examples\adventure_app.rmini
cargo run -- examples\furry_love_game.rmini
```

Chess prototype uses board indexes `0..63`. It is not full chess yet.

## 26. Current Limits

Rust Mini is not full Rust yet.

Missing or incomplete:

- user-defined generics
- trait bounds
- trait objects
- full lifetime syntax
- non-lexical lifetimes
- closures
- full macro system
- Rust's `?` error-propagation operator is not supported yet; use `match` or `.unwrap_or(...)`
- full pattern matching
- destructuring `let`
- real crate/package manager
- full module privacy
- threads and async
- large standard library
- parser recovery
- GUI/window backend

But Rust Mini can already write useful medium programs with Rust-style syntax, ownership, borrowing, structs, enums, match, methods, vectors, loops, formatting, standard helpers, and file I/O.
