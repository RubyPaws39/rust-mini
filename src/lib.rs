pub mod ast;
pub mod borrowcheck;
pub mod bytecode;
pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod typecheck;
pub mod value;

use crate::ast::Program;
use crate::borrowcheck::BorrowChecker;
use crate::error::Result;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::typecheck::TypeChecker;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn parse_source(source: &str) -> Result<Program> {
    let tokens = Lexer::new(source).lex()?;
    Parser::new(tokens).parse_program()
}

pub fn parse_file_with_modules(path: impl AsRef<Path>) -> Result<Program> {
    let path = path.as_ref();
    let mut seen = HashSet::new();
    parse_file_inner(path, &mut seen)
}

fn parse_file_inner(path: &Path, seen: &mut HashSet<PathBuf>) -> Result<Program> {
    let canonical = path.canonicalize().map_err(|e| {
        crate::error::MiniError::runtime(format!("failed to read `{}`: {}", path.display(), e))
    })?;
    if !seen.insert(canonical.clone()) {
        return Ok(Program {
            modules: Vec::new(),
            uses: Vec::new(),
            traits: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            impls: Vec::new(),
            functions: Vec::new(),
        });
    }
    let source = fs::read_to_string(&canonical).map_err(|e| {
        crate::error::MiniError::runtime(format!("failed to read `{}`: {}", canonical.display(), e))
    })?;
    let mut program = parse_source(&source)?;
    let imports = program.modules.clone();
    program.modules.clear();
    program.uses.clear();
    let base = canonical.parent().unwrap_or_else(|| Path::new("."));
    for import in imports {
        let child_path = resolve_module_path(base, &import.path);
        let child = parse_file_inner(&child_path, seen)?;
        merge_program(&mut program, child);
    }
    Ok(program)
}

fn resolve_module_path(base: &Path, import: &str) -> PathBuf {
    if import.ends_with(".rmini") || import.contains('/') || import.contains('\\') {
        return base.join(import);
    }
    let file_path = base.join(format!("{}.rmini", import));
    if file_path.exists() {
        return file_path;
    }
    let mod_path = base.join(import).join("mod.rmini");
    if mod_path.exists() {
        return mod_path;
    }
    if let Ok(cwd) = std::env::current_dir() {
        let lib_file = cwd.join("lib").join(format!("{}.rmini", import));
        if lib_file.exists() {
            return lib_file;
        }
        let lib_mod = cwd.join("lib").join(import).join("mod.rmini");
        if lib_mod.exists() {
            return lib_mod;
        }
        let cwd_file = cwd.join(format!("{}.rmini", import));
        if cwd_file.exists() {
            return cwd_file;
        }
    }
    base.join(import).join("mod.rmini")
}

fn merge_program(target: &mut Program, mut source: Program) {
    target.structs.append(&mut source.structs);
    target.enums.append(&mut source.enums);
    target.traits.append(&mut source.traits);
    target.impls.append(&mut source.impls);
    target.functions.append(&mut source.functions);
}

pub fn check_program(program: &Program) -> Result<()> {
    TypeChecker::new(program).check()?;
    BorrowChecker::new(program).check()
}
