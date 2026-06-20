use rust_mini::borrowcheck::BorrowChecker;
use rust_mini::bytecode::{compile_program, BytecodeVm};
use rust_mini::interpreter::Interpreter;
use rust_mini::lexer::Lexer;
use rust_mini::parse_file_with_modules;
use rust_mini::parser::Parser;
use rust_mini::typecheck::TypeChecker;

fn parse_check_run_vm(source: &str) -> Vec<String> {
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    TypeChecker::new(&program).check().unwrap();
    BorrowChecker::new(&program).check().unwrap();
    let bytecode = compile_program(&program).unwrap();
    BytecodeVm::new(&bytecode).run().unwrap()
}
fn parse_check_run(source: &str) -> Vec<String> {
    parse_check_run_with_args(source, Vec::new())
}

fn parse_check_run_with_args(source: &str, args: Vec<String>) -> Vec<String> {
    parse_check_run_with_io(source, args, Vec::new())
}

fn parse_check_run_with_io(source: &str, args: Vec<String>, input: Vec<String>) -> Vec<String> {
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    TypeChecker::new(&program).check().unwrap();
    BorrowChecker::new(&program).check().unwrap();
    Interpreter::with_args_and_input(&program, args, input)
        .run()
        .unwrap()
}

fn check_error(source: &str) -> String {
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    TypeChecker::new(&program).check().unwrap();
    BorrowChecker::new(&program)
        .check()
        .unwrap_err()
        .to_string()
}

fn runtime_error_without_check(source: &str) -> String {
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    Interpreter::new(&program).run().unwrap_err().to_string()
}

#[test]
fn bytecode_runs_core_programs() {
    assert_eq!(
        parse_check_run_vm(include_str!("../examples/hello.rmini")),
        vec!["Hello from Rust Mini!"]
    );
    assert_eq!(
        parse_check_run_vm(include_str!("../examples/math.rmini")),
        vec!["15"]
    );
    assert_eq!(
        parse_check_run_vm(include_str!("../examples/functions.rmini")),
        vec!["5"]
    );
    assert_eq!(
        parse_check_run_vm(include_str!("../benchmarks/sum_loop.rmini")),
        vec!["19999900000"]
    );
    assert_eq!(
        parse_check_run_vm(include_str!("../benchmarks/fib.rmini")),
        vec!["46368"]
    );
}
#[test]
fn runs_control_flow() {
    let source = include_str!("../examples/control_flow.rmini");
    assert_eq!(parse_check_run(source), vec!["10"]);
}

#[test]
fn runs_break_and_continue() {
    let source = include_str!("../examples/loop_control.rmini");
    assert_eq!(parse_check_run(source), vec!["18"]);
}

#[test]
fn runs_data_types() {
    let source = include_str!("../examples/data_types.rmini");
    assert_eq!(parse_check_run(source), vec!["42", "20", "3", "7"]);
}

#[test]
fn runs_loops_and_mutation() {
    let source = include_str!("../examples/loops_and_mutation.rmini");
    assert_eq!(parse_check_run(source), vec!["24", "4", "25"]);
}

#[test]
fn runs_enums_and_match() {
    let source = include_str!("../examples/enums_match.rmini");
    assert_eq!(parse_check_run(source), vec!["42", "0"]);
}

#[test]
fn runs_methods() {
    let source = include_str!("../examples/methods.rmini");
    assert_eq!(parse_check_run(source), vec!["7", "13", "99"]);
}

#[test]
fn runs_self_methods() {
    let source = include_str!("../examples/self_methods.rmini");
    assert_eq!(parse_check_run(source), vec!["true", "7", "true"]);
}

#[test]
fn runs_float_math() {
    let source = include_str!("../examples/float_math.rmini");
    assert_eq!(parse_check_run(source), vec!["3", "4", "25", "true"]);
}

#[test]
fn runs_module_imports() {
    let program = parse_file_with_modules("examples/modules_demo.rmini").unwrap();
    TypeChecker::new(&program).check().unwrap();
    BorrowChecker::new(&program).check().unwrap();
    assert_eq!(
        Interpreter::new(&program).run().unwrap(),
        vec!["6", "9", "117"]
    );
}

#[test]
fn runs_rust_like_mod_and_use_imports() {
    let program = parse_file_with_modules("examples/imports_demo.rmini").unwrap();
    TypeChecker::new(&program).check().unwrap();
    BorrowChecker::new(&program).check().unwrap();
    assert_eq!(
        Interpreter::new(&program).run().unwrap(),
        vec!["6", "10", "136"]
    );
}

#[test]
fn runs_trait_impls() {
    let source = include_str!("../examples/traits_demo.rmini");
    assert_eq!(parse_check_run(source), vec!["Ruby", "7"]);
}

#[test]
fn runs_calculator_app() {
    let source = include_str!("../examples/calculator_app.rmini");
    assert_eq!(
        parse_check_run(source),
        vec!["Rust Mini Calculator", "16", "12", "42", "42", "37.5"]
    );
}

#[test]
fn runs_interactive_calculator_with_queued_input() {
    let source = include_str!("../examples/interactive_calculator.rmini");
    assert_eq!(
        parse_check_run_with_io(
            source,
            Vec::new(),
            vec!["3".to_string(), "6.5".to_string(), "4.0".to_string()]
        ),
        vec![
            "Rust Mini Interactive Calculator",
            "1 add, 2 subtract, 3 multiply, 4 divide",
            "result",
            "26"
        ]
    );
}

#[test]
fn runs_adventure_app_with_queued_input() {
    let _ = std::fs::remove_file("examples/adventure_save.txt");
    let source = include_str!("../examples/adventure_app.rmini");
    assert_eq!(
        parse_check_run_with_io(source, Vec::new(), vec!["3".to_string()]),
        vec![
            "Rust Mini Adventure",
            "Choose item: 1 sword, 2 potion, 3 relic",
            "You chose",
            "Ancient Relic",
            "final hp",
            "14",
            "final gold",
            "30",
            "final power",
            "44"
        ]
    );
    let written = std::fs::read_to_string("examples/adventure_save.txt").unwrap();
    assert_eq!(written, "adventure complete");
    std::fs::remove_file("examples/adventure_save.txt").unwrap();
}

#[test]
fn runs_vecs() {
    let source = include_str!("../examples/vecs.rmini");
    assert_eq!(parse_check_run(source), vec!["4", "10", "19"]);
}

#[test]
fn runs_string_for_loop_and_format_macros() {
    let source = include_str!("../examples/format_and_string_loop.rmini");
    assert_eq!(
        parse_check_run(source),
        vec![
            "letter r",
            "letter u",
            "letter s",
            "letter t",
            "4 letters, array sum 12"
        ]
    );
}

#[test]
fn runs_std_use_aliases_and_methods() {
    let source = include_str!("../examples/std_io_and_methods.rmini");
    assert_eq!(
        parse_check_run_with_io(source, Vec::new(), vec!["9".to_string()]),
        vec!["len 3", "last 9", "len 2", "Ruby Mini"]
    );
}

#[test]
fn runs_ranges_and_result_methods() {
    let source = include_str!("../examples/range_and_result_methods.rmini");
    assert_eq!(
        parse_check_run(source),
        vec![
            "range sum 10",
            "ok true",
            "value 42",
            "err true",
            "fallback 9",
            "some true",
            "item 7",
            "empty true"
        ]
    );
}

#[test]
fn runs_string_methods() {
    let source = include_str!("../examples/string_methods.rmini");
    assert_eq!(
        parse_check_run(source),
        vec![
            "Rust Mini",
            "RUST MINI",
            "ruby mini",
            "true",
            "true",
            "true"
        ]
    );
}

#[test]
fn runs_file_io_and_string_len() {
    let source = include_str!("../examples/io.rmini");
    assert_eq!(
        parse_check_run(source),
        vec!["Rust Mini file IO works.\n", "25"]
    );
    let written = std::fs::read_to_string("examples/io_out.txt").unwrap();
    assert_eq!(written, "created by Rust Mini");
    std::fs::remove_file("examples/io_out.txt").unwrap();
}

#[test]
fn runs_host_std_helpers() {
    std::env::set_var("RUST_MINI_TEST_ENV", "env-ok");
    let source = include_str!("../examples/host_std.rmini");
    assert_eq!(
        parse_check_run_with_args(source, vec!["first".to_string(), "second".to_string()]),
        vec!["hello world", "true", "2", "env-ok", "true"]
    );
    std::env::remove_var("RUST_MINI_TEST_ENV");
}

#[test]
fn runs_terminal_helpers() {
    let source = r#"
fn main() {
    clear();
    sleep_ms(0);
    print(rand_i64(5, 5));
    print(color("hi", "red"));
}
"#;
    assert_eq!(parse_check_run(source), vec!["5", "\u{1b}[31mhi\u{1b}[0m"]);
}

#[test]
fn polls_nonblocking_key_input() {
    let source = r#"
fn main() {
    let key: String = game::read_key();
    print(len(key) >= 0);
}
"#;
    assert_eq!(parse_check_run(source), vec!["true"]);
}

#[test]
fn runs_result_and_option_demo() {
    let source = include_str!("../examples/result_demo.rmini");
    assert_eq!(
        parse_check_run(source),
        vec!["42", "cannot parse `oops` as i64", "0", "7", "99"]
    );
}

#[test]
fn runs_imported_library_demo() {
    let _ = std::fs::remove_file("examples/library_demo_out.txt");
    let program = parse_file_with_modules("examples/library_demo.rmini").unwrap();
    TypeChecker::new(&program).check().unwrap();
    BorrowChecker::new(&program).check().unwrap();
    assert_eq!(
        Interpreter::with_args_and_input(&program, Vec::new(), vec!["41".to_string()])
            .run()
            .unwrap(),
        vec![
            "\u{1b}[92mImported libraries loaded\u{1b}[0m",
            "42",
            "library demo complete"
        ]
    );
    std::fs::remove_file("examples/library_demo_out.txt").unwrap();
}

#[test]
fn runs_turtle_demo() {
    let _ = std::fs::remove_file("examples/turtle_demo.svg");
    let program = parse_file_with_modules("examples/turtle_demo.rmini").unwrap();
    TypeChecker::new(&program).check().unwrap();
    BorrowChecker::new(&program).check().unwrap();
    assert_eq!(
        Interpreter::new(&program).run().unwrap(),
        vec!["wrote examples/turtle_demo.svg"]
    );
    let svg = std::fs::read_to_string("examples/turtle_demo.svg").unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("hotpink"));
    assert!(svg.contains("deepskyblue"));
    std::fs::remove_file("examples/turtle_demo.svg").unwrap();
}

#[test]
fn logo_forward_draws_line_and_save_writes_svg() {
    let _ = std::fs::remove_file("examples/logo_test_line.svg");
    let source = r#"
fn main() {
    logo::clear();
    logo::forward(100);
    logo::save("examples/logo_test_line.svg");
}
"#;
    assert_eq!(parse_check_run(source), Vec::<String>::new());
    let svg = std::fs::read_to_string("examples/logo_test_line.svg").unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("<line"));
    assert!(svg.contains("x1=\"250.00\""));
    assert!(svg.contains("x2=\"350.00\""));
    std::fs::remove_file("examples/logo_test_line.svg").unwrap();
}

#[test]
fn logo_right_and_left_change_direction() {
    let _ = std::fs::remove_file("examples/logo_test_turns.svg");
    let source = r#"
fn main() {
    logo::clear();
    logo::right(90);
    logo::forward(50);
    logo::left(90);
    logo::forward(50);
    logo::save("examples/logo_test_turns.svg");
}
"#;
    assert_eq!(parse_check_run(source), Vec::<String>::new());
    let svg = std::fs::read_to_string("examples/logo_test_turns.svg").unwrap();
    assert!(svg.contains("y2=\"300.00\""));
    assert!(svg.contains("x2=\"300.00\""));
    std::fs::remove_file("examples/logo_test_turns.svg").unwrap();
}

#[test]
fn logo_pen_up_prevents_line_creation() {
    let _ = std::fs::remove_file("examples/logo_test_pen.svg");
    let source = r#"
fn main() {
    logo::clear();
    logo::pen_up();
    logo::forward(100);
    logo::pen_down();
    logo::forward(50);
    logo::save("examples/logo_test_pen.svg");
}
"#;
    assert_eq!(parse_check_run(source), Vec::<String>::new());
    let svg = std::fs::read_to_string("examples/logo_test_pen.svg").unwrap();
    assert_eq!(svg.matches("<line").count(), 1);
    assert!(svg.contains("x1=\"350.00\""));
    assert!(svg.contains("x2=\"400.00\""));
    std::fs::remove_file("examples/logo_test_pen.svg").unwrap();
}

#[test]
fn logo_pen_color_changes_future_lines() {
    let _ = std::fs::remove_file("examples/logo_test_color.svg");
    let source = r#"
fn main() {
    logo::clear();
    logo::pen_color("red");
    logo::forward(30);
    logo::pen_color("blue");
    logo::forward(30);
    logo::save("examples/logo_test_color.svg");
}
"#;
    assert_eq!(parse_check_run(source), Vec::<String>::new());
    let svg = std::fs::read_to_string("examples/logo_test_color.svg").unwrap();
    assert!(svg.contains("stroke=\"red\""));
    assert!(svg.contains("stroke=\"blue\""));
    std::fs::remove_file("examples/logo_test_color.svg").unwrap();
}

#[test]
fn logo_position_home_heading_circle_width_background_and_size_work() {
    let _ = std::fs::remove_file("examples/logo_test_state.svg");
    let source = r#"
fn main() {
    logo::clear();
    logo::background("mintcream");
    logo::width(7);
    logo::set_position(100, 100);
    logo::set_heading(90);
    print(logo::heading());
    logo::circle(25);
    logo::forward(50);
    logo::home();
    logo::save_with_size("examples/logo_test_state.svg", 320, 240);
}
"#;
    assert_eq!(parse_check_run(source), vec!["90"]);
    let svg = std::fs::read_to_string("examples/logo_test_state.svg").unwrap();
    assert!(svg.contains("width=\"320\""));
    assert!(svg.contains("height=\"240\""));
    assert!(svg.contains("fill=\"mintcream\""));
    assert!(svg.contains("<circle"));
    assert!(svg.contains("r=\"25.00\""));
    assert!(svg.contains("stroke-width=\"7\""));
    assert!(svg.contains("x1=\"250.00\""));
    assert!(svg.contains("x2=\"100.00\""));
    std::fs::remove_file("examples/logo_test_state.svg").unwrap();
}

#[test]
fn logo_invalid_runtime_argument_is_friendly() {
    let source = r#"
fn main() {
    logo::forward("far");
}
"#;
    let err = runtime_error_without_check(source);
    assert!(err.contains("function `logo_forward` expects i64 distance/degrees"));
}

#[test]
fn runs_rpg_demo() {
    let _ = std::fs::remove_file("examples/rpg_demo_out.txt");
    let source = include_str!("../examples/rpg_demo.rmini");
    assert_eq!(parse_check_run(source), vec!["11", "16", "5"]);
    let written = std::fs::read_to_string("examples/rpg_demo_out.txt").unwrap();
    assert_eq!(written, "rpg demo complete");
    std::fs::remove_file("examples/rpg_demo_out.txt").unwrap();
}

#[test]
fn checks_all_success_examples() {
    for path in [
        "examples/hello.rmini",
        "examples/math.rmini",
        "examples/functions.rmini",
        "examples/borrow_ok.rmini",
        "examples/lifetimes.rmini",
        "examples/destructuring.rmini",
        "examples/match_patterns.rmini",
        "examples/question_operator.rmini",
        "examples/control_flow.rmini",
        "examples/loop_control.rmini",
        "examples/data_types.rmini",
        "examples/loops_and_mutation.rmini",
        "examples/enums_match.rmini",
        "examples/methods.rmini",
        "examples/self_methods.rmini",
        "examples/float_math.rmini",
        "examples/modules_demo.rmini",
        "examples/imports_demo.rmini",
        "examples/traits_demo.rmini",
        "examples/calculator_app.rmini",
        "examples/interactive_calculator.rmini",
        "examples/animated_pong.rmini",
        "examples/adventure_app.rmini",
        "examples/furry_love_game.rmini",
        "examples/format_and_string_loop.rmini",
        "examples/std_io_and_methods.rmini",
        "examples/range_and_result_methods.rmini",
        "examples/string_methods.rmini",
        "examples/vecs.rmini",
        "examples/io.rmini",
        "examples/host_std.rmini",
        "examples/result_demo.rmini",
        "examples/library_demo.rmini",
        "examples/turtle_demo.rmini",
        "examples/logo_square.rmini",
        "examples/logo_triangle.rmini",
        "examples/logo_spiral.rmini",
        "examples/logo_flower.rmini",
        "examples/logo_shiba_head.rmini",
        "examples/chess_prototype.rmini",
        "examples/rpg_demo.rmini",
    ] {
        let program = if path == "examples/modules_demo.rmini"
            || path == "examples/imports_demo.rmini"
            || path == "examples/library_demo.rmini"
            || path == "examples/turtle_demo.rmini"
            || path == "examples/chess_prototype.rmini"
            || path == "examples/animated_pong.rmini"
        {
            parse_file_with_modules(path).unwrap()
        } else {
            let source = std::fs::read_to_string(path).unwrap();
            let tokens = Lexer::new(&source).lex().unwrap();
            Parser::new(tokens).parse_program().unwrap()
        };
        TypeChecker::new(&program).check().unwrap();
        BorrowChecker::new(&program).check().unwrap();
    }
}

#[test]
fn rejects_vec_push_type_mismatch() {
    let source = "fn main(){ let mut xs: Vec<i64> = vec![1]; push(&mut xs, true); }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("expected `I64`, found `Bool`"));
}

#[test]
fn rejects_reference_return() {
    let source = "fn bad(x: &i64) -> &i64 { return x; } fn main(){ let x: i64 = 1; }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("cannot return a reference"));
}

#[test]
fn rejects_reference_in_struct_field() {
    let source = "struct Holder { value: &i64 } fn main(){ let x: i64 = 1; }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("cannot store a reference"));
}

#[test]
fn rejects_reference_in_vec() {
    let source = "fn main(){ let x: i64 = 1; let xs = vec![&x]; }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("vecs cannot store references"));
}

#[test]
fn rejects_mixed_numeric_arithmetic() {
    let source = "fn main(){ let x: f64 = 1.0 + 2; }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("expected `F64`, found `I64`"));
}

#[test]
fn rejects_unknown_method() {
    let source = "struct Point { x: i64 } fn main(){ let p: Point = Point { x: 1 }; p.nope(); }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("unknown method `nope`"));
}

#[test]
fn rejects_incomplete_trait_impl() {
    let source = "trait Named { fn name(&self) -> String; } struct Player { name: String } impl Named for Player { } fn main(){ }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("missing method `name`"));
}

#[test]
fn rejects_non_exhaustive_match() {
    let source = "enum Maybe { Some(i64), None } fn main(){ let x: Maybe = Maybe::Some(1); let y: i64 = match x { Maybe::Some(v) => v, }; }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("non-exhaustive match"));
}

#[test]
fn rejects_match_arm_type_mismatch() {
    let source = "enum Maybe { Some(i64), None } fn main(){ let x: Maybe = Maybe::None; let y: i64 = match x { Maybe::Some(v) => v, Maybe::None => false, }; }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("expected `I64`, found `Bool`"));
}

#[test]
fn rejects_for_over_non_array() {
    let tokens = Lexer::new("fn main(){ for n in 1 { print(n); } }")
        .lex()
        .unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("`for` expects an array"));
}

#[test]
fn rejects_index_assignment_type_mismatch() {
    let tokens = Lexer::new("fn main(){ let mut xs: [i64; 2] = [1, 2]; xs[0] = true; }")
        .lex()
        .unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("expected `I64`, found `Bool`"));
}

#[test]
fn rejects_array_element_mismatch() {
    let tokens = Lexer::new("fn main(){ let xs = [1, true]; }")
        .lex()
        .unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("expected `I64`, found `Bool`"));
}

#[test]
fn rejects_unknown_struct_field() {
    let source = "struct Point { x: i64 } fn main(){ let p: Point = Point { y: 1 }; }";
    let tokens = Lexer::new(source).lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("missing field `x`"));
}

#[test]
fn runs_block_tail_values_and_shadowing() {
    let source = r#"
fn main() {
    let x: i64 = 4;
    let x: i64 = {
        let y: i64 = x + 1;
        y * 2
    };
    print(x);
}
"#;
    assert_eq!(parse_check_run(source), vec!["10"]);
}

#[test]
fn rejects_break_outside_loop() {
    let tokens = Lexer::new("fn main(){ break; }").lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("`break` outside loop"));
}

#[test]
fn runs_function_call_and_return() {
    let source = include_str!("../examples/functions.rmini");
    assert_eq!(parse_check_run(source), vec!["5"]);
}

#[test]
fn runs_mutable_reference_call() {
    let source = include_str!("../examples/borrow_ok.rmini");
    assert_eq!(parse_check_run(source), vec!["9"]);
}

#[test]
fn runs_literal_tuple_and_binding_match_patterns() {
    let source = include_str!("../examples/match_patterns.rmini");
    assert_eq!(parse_check_run(source), vec!["zero", "many", "17", "yes"]);
}

#[test]
fn runs_destructuring_let() {
    let source = include_str!("../examples/destructuring.rmini");
    assert_eq!(parse_check_run(source), vec!["5", "33", "7"]);
}

#[test]
fn rejects_destructuring_tuple_arity_mismatch() {
    let tokens = Lexer::new("fn main(){ let (a, b) = (1, 2, 3); }")
        .lex()
        .unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("tuple pattern has 2 fields, value has 3"));
}

#[test]
fn runs_named_lifetime_reference_params() {
    let source = include_str!("../examples/lifetimes.rmini");
    assert_eq!(parse_check_run(source), vec!["&hp", "15"]);
}

#[test]
fn runs_question_operator_for_result_and_option() {
    let source = include_str!("../examples/question_operator.rmini");
    assert_eq!(
        parse_check_run(source),
        vec![
            "Result::Ok(6)",
            "Result::Err(cannot parse `nope` as i64)",
            "Option::Some(7)",
            "Option::None",
        ]
    );
}

#[test]
fn rejects_question_on_plain_value() {
    let tokens = Lexer::new("fn main(){ let x: i64 = 5?; }").lex().unwrap();
    let program = Parser::new(tokens).parse_program().unwrap();
    let err = TypeChecker::new(&program).check().unwrap_err().to_string();
    assert!(err.contains("`?` expects Option or Result"));
}

#[test]
fn rejects_undeclared_lifetime() {
    let tokens = Lexer::new("fn show(value: &'a i64) {}").lex().unwrap();
    let err = Parser::new(tokens).parse_program().unwrap_err().to_string();
    assert!(err.contains("undeclared lifetime `'a`"));
}

#[test]
fn rejects_duplicate_lifetime_parameter() {
    let tokens = Lexer::new("fn show<'a, 'a>(value: &'a i64) {}")
        .lex()
        .unwrap();
    let err = Parser::new(tokens).parse_program().unwrap_err().to_string();
    assert!(err.contains("duplicate lifetime parameter `'a`"));
}

#[test]
fn rejects_moved_string_use() {
    let source = include_str!("../examples/move_error.rmini");
    assert!(check_error(source).contains("use of moved value `name`"));
}

#[test]
fn rejects_borrow_conflict() {
    let source = include_str!("../examples/borrow_error.rmini");
    assert!(
        check_error(source).contains("cannot mutably borrow `x` while it is immutably borrowed")
    );
}

#[test]
fn releases_borrows_at_block_end() {
    let source = r#"
fn main() {
    let mut x: i64 = 5;
    {
        let a = &x;
        print(a);
    };
    let b = &mut x;
    *b = *b + 1;
    print(x);
}
"#;
    assert_eq!(parse_check_run(source), vec!["&x", "6"]);
}
