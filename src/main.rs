use rust_mini::borrowcheck::BorrowChecker;
use rust_mini::interpreter::Interpreter;
use rust_mini::lexer::Lexer;
use rust_mini::parse_file_with_modules;
use rust_mini::parser::Parser;
use rust_mini::typecheck::TypeChecker;
use std::env;
use std::fs;
use std::process;

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_usage();
        process::exit(2);
    }
    let split_at = args.iter().position(|arg| arg == "--");
    let (cli_args, program_args) = if let Some(idx) = split_at {
        (args[..idx].to_vec(), args[idx + 1..].to_vec())
    } else {
        (args, Vec::new())
    };
    if cli_args.is_empty() {
        print_usage();
        process::exit(2);
    }
    if cli_args.len() == 1 && (cli_args[0] == "--version" || cli_args[0] == "-V") {
        println!("rust_mini {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if cli_args.len() == 1 && (cli_args[0] == "--help" || cli_args[0] == "-h") {
        print_usage();
        return Ok(());
    }

    let (mode, path) = if cli_args.len() == 2 && cli_args[0] == "run" {
        ("--run", cli_args[1].as_str())
    } else if cli_args.len() == 2 && cli_args[0].starts_with("--") {
        (cli_args[0].as_str(), cli_args[1].as_str())
    } else {
        ("--run", cli_args[0].as_str())
    };
    if !matches!(mode, "--run" | "--tokens" | "--ast" | "--check") {
        return Err(format!("runtime error: unknown option `{}`", mode));
    }

    let source = fs::read_to_string(path)
        .map_err(|e| format!("runtime error: failed to read `{}`: {}", path, e))?;
    let tokens = Lexer::new(&source)
        .lex()
        .map_err(|e| e.render_with_source(&source))?;
    if mode == "--tokens" {
        for token in tokens {
            println!("{:?}", token);
        }
        return Ok(());
    }

    let program = if mode == "--tokens" || mode == "--ast" {
        Parser::new(tokens)
            .parse_program()
            .map_err(|e| e.render_with_source(&source))?
    } else {
        parse_file_with_modules(path).map_err(|e| e.render_with_source(&source))?
    };
    if mode == "--ast" {
        println!("{:#?}", program);
        return Ok(());
    }

    TypeChecker::new(&program)
        .check()
        .map_err(|e| e.render_with_source(&source))?;
    BorrowChecker::new(&program)
        .check()
        .map_err(|e| e.render_with_source(&source))?;
    if mode == "--check" {
        println!("check ok");
        return Ok(());
    }

    for line in Interpreter::with_args_and_live_output(&program, program_args)
        .run()
        .map_err(|e| e.render_with_source(&source))?
    {
        println!("{}", line);
    }
    Ok(())
}

fn print_usage() {
    eprintln!("rust_mini {}", env!("CARGO_PKG_VERSION"));
    eprintln!("usage: rust_mini [--tokens|--ast|--check|--version|--help] <file.rmini>");
    eprintln!("       rust_mini run <file.rmini>");
    eprintln!("       rust_mini <file.rmini> -- [program args...]");
}
