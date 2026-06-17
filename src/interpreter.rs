use crate::ast::*;
use crate::error::{MiniError, Result};
use crate::value::{RefValue, Value};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
#[link(name = "msvcrt")]
extern "C" {
    fn _kbhit() -> i32;
    fn _getch() -> i32;
}

#[derive(Debug, Clone)]
struct RuntimeVar {
    value: Value,
    mutable: bool,
}

enum Flow {
    Value(Value),
    Return(Value),
    Break,
    Continue,
}

pub struct Interpreter<'a> {
    functions: HashMap<String, &'a Function>,
    frames: Vec<HashMap<String, RuntimeVar>>,
    output: Vec<String>,
    args: Vec<String>,
    input: VecDeque<String>,
    rng_state: u64,
    live_output: bool,
}

impl<'a> Interpreter<'a> {
    pub fn new(program: &'a Program) -> Self {
        Self::with_args(program, Vec::new())
    }

    pub fn with_args(program: &'a Program, args: Vec<String>) -> Self {
        Self::with_args_and_input(program, args, Vec::new())
    }

    pub fn with_args_and_input(
        program: &'a Program,
        args: Vec<String>,
        input: Vec<String>,
    ) -> Self {
        let rng_state = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x5eed);
        let mut functions = HashMap::new();
        for function in &program.functions {
            functions.insert(function.name.clone(), function);
        }
        for block in &program.impls {
            for method in &block.methods {
                functions.insert(method.name.clone(), method);
            }
        }
        Self {
            functions,
            frames: Vec::new(),
            output: Vec::new(),
            args,
            input: input.into(),
            rng_state,
            live_output: false,
        }
    }

    pub fn with_args_and_live_output(program: &'a Program, args: Vec<String>) -> Self {
        let mut interpreter = Self::with_args(program, args);
        interpreter.live_output = true;
        interpreter
    }

    pub fn run(mut self) -> Result<Vec<String>> {
        self.call_function("main", Vec::new())?;
        Ok(self.output)
    }

    fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value> {
        let name = builtin_alias(name);
        if name == "print" {
            if args.len() != 1 {
                return Err(MiniError::runtime("function `print` expects 1 argument"));
            }
            if self.live_output {
                println!("{}", args[0]);
                io::stdout()
                    .flush()
                    .map_err(|e| MiniError::runtime(format!("failed to flush stdout: {}", e)))?;
            } else {
                self.output.push(args[0].to_string());
            }
            return Ok(Value::Unit);
        }
        if name == "__format_macro" || name == "__print_macro" || name == "__println_macro" {
            let text = format_macro_values(&args)?;
            if name == "__format_macro" {
                return Ok(Value::String(text));
            }
            return self.call_function("print", vec![Value::String(text)]);
        }
        if name == "len" {
            if args.len() != 1 {
                return Err(MiniError::runtime("function `len` expects 1 argument"));
            }
            return match &args[0] {
                Value::String(text) => Ok(Value::Int(text.chars().count() as i64)),
                Value::Array(items) | Value::Vec(items) => Ok(Value::Int(items.len() as i64)),
                _ => Err(MiniError::runtime(
                    "function `len` expects String, array, or vec",
                )),
            };
        }
        if name == "args" {
            if !args.is_empty() {
                return Err(MiniError::runtime("function `args` expects 0 arguments"));
            }
            return Ok(Value::Vec(
                self.args.iter().cloned().map(Value::String).collect(),
            ));
        }
        if name == "clock_ms" {
            if !args.is_empty() {
                return Err(MiniError::runtime(
                    "function `clock_ms` expects 0 arguments",
                ));
            }
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| MiniError::runtime(format!("system clock error: {}", e)))?
                .as_millis();
            let millis = i64::try_from(millis)
                .map_err(|_| MiniError::runtime("clock value does not fit i64"))?;
            return Ok(Value::Int(millis));
        }
        if name == "clear" {
            if !args.is_empty() {
                return Err(MiniError::runtime("function `clear` expects 0 arguments"));
            }
            print!("\x1b[2J\x1b[H");
            io::stdout()
                .flush()
                .map_err(|e| MiniError::runtime(format!("failed to flush stdout: {}", e)))?;
            return Ok(Value::Unit);
        }
        if name == "read_key" {
            if !args.is_empty() {
                return Err(MiniError::runtime(
                    "function `read_key` expects 0 arguments",
                ));
            }
            return Ok(Value::String(read_key_nonblocking()));
        }
        if name == "sleep_ms" {
            if args.len() != 1 {
                return Err(MiniError::runtime("function `sleep_ms` expects 1 argument"));
            }
            let Value::Int(ms) = args[0] else {
                return Err(MiniError::runtime("function `sleep_ms` expects i64"));
            };
            if ms > 0 {
                thread::sleep(Duration::from_millis(ms as u64));
            }
            return Ok(Value::Unit);
        }
        if name == "rand_i64" {
            if args.len() != 2 {
                return Err(MiniError::runtime(
                    "function `rand_i64` expects 2 arguments",
                ));
            }
            let Value::Int(min) = args[0] else {
                return Err(MiniError::runtime("function `rand_i64` expects i64 min"));
            };
            let Value::Int(max) = args[1] else {
                return Err(MiniError::runtime("function `rand_i64` expects i64 max"));
            };
            if min > max {
                return Err(MiniError::runtime("function `rand_i64` expects min <= max"));
            }
            self.rng_state = self
                .rng_state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let span = (max - min + 1) as u64;
            return Ok(Value::Int(min + (self.rng_state % span) as i64));
        }
        if name == "color" {
            if args.len() != 2 {
                return Err(MiniError::runtime("function `color` expects 2 arguments"));
            }
            let Value::String(text) = &args[0] else {
                return Err(MiniError::runtime("function `color` expects String text"));
            };
            let Value::String(color) = &args[1] else {
                return Err(MiniError::runtime("function `color` expects String color"));
            };
            let code = match color.as_str() {
                "black" => "30",
                "red" => "31",
                "green" => "32",
                "yellow" => "33",
                "blue" => "34",
                "magenta" => "35",
                "cyan" => "36",
                "white" => "37",
                "bright_red" => "91",
                "bright_green" => "92",
                "bright_yellow" => "93",
                "bright_blue" => "94",
                "bright_magenta" => "95",
                "bright_cyan" => "96",
                _ => "0",
            };
            return Ok(Value::String(format!("\x1b[{}m{}\x1b[0m", code, text)));
        }
        if name == "input" {
            if args.len() != 1 {
                return Err(MiniError::runtime("function `input` expects 1 argument"));
            }
            let Value::String(prompt) = &args[0] else {
                return Err(MiniError::runtime("function `input` expects String prompt"));
            };
            if let Some(line) = self.input.pop_front() {
                return Ok(Value::String(line));
            }
            if !self.live_output {
                for line in self.output.drain(..) {
                    println!("{}", line);
                }
            }
            print!("{}", prompt);
            io::stdout()
                .flush()
                .map_err(|e| MiniError::runtime(format!("failed to flush stdout: {}", e)))?;
            let mut line = String::new();
            io::stdin()
                .read_line(&mut line)
                .map_err(|e| MiniError::runtime(format!("failed to read input: {}", e)))?;
            return Ok(Value::String(
                line.trim_end_matches(&['\r', '\n'][..]).to_string(),
            ));
        }
        if name == "read_i64_alias" {
            if args.len() != 2 {
                return Err(MiniError::runtime(
                    "function `std::io::read_i64` expects 2 arguments",
                ));
            }
            let Value::String(prompt) = &args[0] else {
                return Err(MiniError::runtime(
                    "function `std::io::read_i64` expects String prompt",
                ));
            };
            let Value::Int(default) = args[1] else {
                return Err(MiniError::runtime(
                    "function `std::io::read_i64` expects i64 default",
                ));
            };
            let input = self.call_function("input", vec![Value::String(prompt.clone())])?;
            let parsed = self.call_function("parse_i64", vec![input])?;
            return self.call_function("unwrap_or", vec![parsed, Value::Int(default)]);
        }
        if name == "read_f64_alias" {
            if args.len() != 2 {
                return Err(MiniError::runtime(
                    "function `std::io::read_f64` expects 2 arguments",
                ));
            }
            let Value::String(prompt) = &args[0] else {
                return Err(MiniError::runtime(
                    "function `std::io::read_f64` expects String prompt",
                ));
            };
            let Value::Float(default) = args[1] else {
                return Err(MiniError::runtime(
                    "function `std::io::read_f64` expects f64 default",
                ));
            };
            let input = self.call_function("input", vec![Value::String(prompt.clone())])?;
            let parsed = self.call_function("parse_f64", vec![input])?;
            return self.call_function("unwrap_or", vec![parsed, Value::Float(default)]);
        }
        if name == "parse_f64" {
            if args.len() != 1 {
                return Err(MiniError::runtime(
                    "function `parse_f64` expects 1 argument",
                ));
            }
            let Value::String(text) = &args[0] else {
                return Err(MiniError::runtime("function `parse_f64` expects String"));
            };
            return Ok(match text.parse::<f64>() {
                Ok(value) => result_ok(Value::Float(value.to_bits())),
                Err(_) => result_err(format!("cannot parse `{}` as f64", text)),
            });
        }
        if name == "parse_i64" {
            if args.len() != 1 {
                return Err(MiniError::runtime(
                    "function `parse_i64` expects 1 argument",
                ));
            }
            let Value::String(text) = &args[0] else {
                return Err(MiniError::runtime("function `parse_i64` expects String"));
            };
            return Ok(match text.parse::<i64>() {
                Ok(value) => result_ok(Value::Int(value)),
                Err(_) => result_err(format!("cannot parse `{}` as i64", text)),
            });
        }
        if name == "unwrap_or" {
            if args.len() != 2 {
                return Err(MiniError::runtime(
                    "function `unwrap_or` expects 2 arguments",
                ));
            }
            return match &args[0] {
                Value::Enum {
                    enum_name,
                    variant,
                    value,
                } if (enum_name == "Result" && variant == "Ok")
                    || (enum_name == "Option" && variant == "Some") =>
                {
                    Ok(value.as_deref().cloned().unwrap_or(Value::Unit))
                }
                Value::Enum { .. } => Ok(args[1].clone()),
                _ => Err(MiniError::runtime(
                    "function `unwrap_or` expects Option or Result",
                )),
            };
        }
        if name == "concat" {
            if args.len() != 2 {
                return Err(MiniError::runtime("function `concat` expects 2 arguments"));
            }
            let Value::String(left) = &args[0] else {
                return Err(MiniError::runtime(
                    "function `concat` expects String left value",
                ));
            };
            let Value::String(right) = &args[1] else {
                return Err(MiniError::runtime(
                    "function `concat` expects String right value",
                ));
            };
            return Ok(Value::String(format!("{}{}", left, right)));
        }
        if name == "contains" {
            if args.len() != 2 {
                return Err(MiniError::runtime(
                    "function `contains` expects 2 arguments",
                ));
            }
            let Value::String(haystack) = &args[0] else {
                return Err(MiniError::runtime(
                    "function `contains` expects String haystack",
                ));
            };
            let Value::String(needle) = &args[1] else {
                return Err(MiniError::runtime(
                    "function `contains` expects String needle",
                ));
            };
            return Ok(Value::Bool(haystack.contains(needle)));
        }
        if name == "env" {
            if args.len() != 1 {
                return Err(MiniError::runtime("function `env` expects 1 argument"));
            }
            let Value::String(name) = &args[0] else {
                return Err(MiniError::runtime("function `env` expects String name"));
            };
            return Ok(Value::String(env::var(name).unwrap_or_default()));
        }
        if name == "read_file" {
            if args.len() != 1 {
                return Err(MiniError::runtime(
                    "function `read_file` expects 1 argument",
                ));
            }
            let Value::String(path) = &args[0] else {
                return Err(MiniError::runtime(
                    "function `read_file` expects String path",
                ));
            };
            return Ok(match fs::read_to_string(path) {
                Ok(text) => result_ok(Value::String(text)),
                Err(e) => result_err(format!("failed to read `{}`: {}", path, e)),
            });
        }
        if name == "write_file" {
            if args.len() != 2 {
                return Err(MiniError::runtime(
                    "function `write_file` expects 2 arguments",
                ));
            }
            let Value::String(path) = &args[0] else {
                return Err(MiniError::runtime(
                    "function `write_file` expects String path",
                ));
            };
            let Value::String(body) = &args[1] else {
                return Err(MiniError::runtime(
                    "function `write_file` expects String body",
                ));
            };
            fs::write(path, body)
                .map_err(|e| MiniError::runtime(format!("failed to write `{}`: {}", path, e)))?;
            return Ok(Value::Unit);
        }
        if name == "push" {
            if args.len() != 2 {
                return Err(MiniError::runtime("function `push` expects 2 arguments"));
            }
            let Value::Ref(reference) = &args[0] else {
                return Err(MiniError::runtime(
                    "function `push` expects mutable vec reference",
                ));
            };
            if !reference.mutable {
                return Err(MiniError::runtime(
                    "function `push` expects mutable vec reference",
                ));
            }
            let var = self.frames[reference.frame]
                .get_mut(&reference.name)
                .ok_or_else(|| {
                    MiniError::runtime(format!("dangling reference `{}`", reference.name))
                })?;
            let Value::Vec(items) = &mut var.value else {
                return Err(MiniError::runtime("function `push` expects vec"));
            };
            items.push(args[1].clone());
            return Ok(Value::Unit);
        }
        let function = self
            .functions
            .get(name)
            .ok_or_else(|| MiniError::runtime(format!("function `{}` not found", name)))?;
        if args.len() != function.params.len() {
            return Err(MiniError::runtime(format!(
                "function `{}` expects {} arguments",
                name,
                function.params.len()
            )));
        }
        let frame_index = self.frames.len();
        let mut frame = HashMap::new();
        for (param, value) in function.params.iter().zip(args) {
            frame.insert(
                param.name.clone(),
                RuntimeVar {
                    value,
                    mutable: true,
                },
            );
        }
        self.frames.push(frame);
        let flow = self.eval_block(&function.body)?;
        self.frames.remove(frame_index);
        match flow {
            Flow::Value(value) | Flow::Return(value) => Ok(value),
            Flow::Break | Flow::Continue => {
                Err(MiniError::runtime("loop control escaped function"))
            }
        }
    }

    fn eval_block(&mut self, block: &Block) -> Result<Flow> {
        self.frames.push(HashMap::new());
        for stmt in &block.statements {
            match self.eval_statement(stmt)? {
                Flow::Value(_) => {}
                ret @ (Flow::Return(_) | Flow::Break | Flow::Continue) => {
                    self.frames.pop();
                    return Ok(ret);
                }
            }
        }
        let flow = if let Some(tail) = &block.tail {
            match &**tail {
                Expression::Block(_) | Expression::If { .. } => self.eval_expr_statement(tail)?,
                _ => Flow::Value(self.eval_expr(tail)?),
            }
        } else {
            Flow::Value(Value::Unit)
        };
        self.frames.pop();
        Ok(flow)
    }

    fn eval_statement(&mut self, stmt: &Statement) -> Result<Flow> {
        match stmt {
            Statement::Let {
                name,
                mutable,
                value,
                ..
            } => {
                let value = self.eval_expr(value)?;
                self.frames.last_mut().unwrap().insert(
                    name.clone(),
                    RuntimeVar {
                        value,
                        mutable: *mutable,
                    },
                );
                Ok(Flow::Value(Value::Unit))
            }
            Statement::Assign { target, value, .. } => {
                let value = self.eval_expr(value)?;
                self.assign_target(target, value)?;
                Ok(Flow::Value(Value::Unit))
            }
            Statement::Expr(expr) => self.eval_expr_statement(expr),
            Statement::Return { value, .. } => {
                let value = if let Some(value) = value {
                    self.eval_expr(value)?
                } else {
                    Value::Unit
                };
                Ok(Flow::Return(value))
            }
            Statement::Break { .. } => Ok(Flow::Break),
            Statement::Continue { .. } => Ok(Flow::Continue),
            Statement::While {
                condition, body, ..
            } => {
                while self.eval_expr(condition)? == Value::Bool(true) {
                    match self.eval_block(body)? {
                        Flow::Value(_) | Flow::Continue => {}
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                    }
                }
                Ok(Flow::Value(Value::Unit))
            }
            Statement::Loop { body, .. } => {
                loop {
                    match self.eval_block(body)? {
                        Flow::Value(_) | Flow::Continue => {}
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                    }
                }
                Ok(Flow::Value(Value::Unit))
            }
            Statement::For {
                name,
                iterable,
                body,
                ..
            } => {
                let iterable = self.eval_expr(iterable)?;
                let items = match iterable {
                    Value::Array(items) | Value::Vec(items) => items,
                    Value::String(text) => text
                        .chars()
                        .map(|ch| Value::String(ch.to_string()))
                        .collect(),
                    Value::Range(start, end) => (start..end).map(Value::Int).collect(),
                    _ => {
                        return Err(MiniError::runtime(
                            "`for` expects array, vec, String, or range",
                        ))
                    }
                };
                for item in items {
                    self.frames.push(HashMap::new());
                    self.frames.last_mut().unwrap().insert(
                        name.clone(),
                        RuntimeVar {
                            value: item,
                            mutable: false,
                        },
                    );
                    let flow = self.eval_block(body)?;
                    self.frames.pop();
                    match flow {
                        Flow::Value(_) | Flow::Continue => {}
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                    }
                }
                Ok(Flow::Value(Value::Unit))
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expression) -> Result<Value> {
        match expr {
            Expression::Int(v, _) => Ok(Value::Int(*v)),
            Expression::Float(v, _) => Ok(Value::Float(*v)),
            Expression::Bool(v, _) => Ok(Value::Bool(*v)),
            Expression::String(v, _) => Ok(Value::String(v.clone())),
            Expression::Range { start, end, .. } => {
                let start = self.eval_expr(start)?;
                let end = self.eval_expr(end)?;
                let (Value::Int(start), Value::Int(end)) = (start, end) else {
                    return Err(MiniError::runtime("range bounds must be i64"));
                };
                Ok(Value::Range(start, end))
            }
            Expression::Tuple(items, _) => Ok(Value::Tuple(
                items
                    .iter()
                    .map(|item| self.eval_expr(item))
                    .collect::<Result<Vec<_>>>()?,
            )),
            Expression::Array(items, _) => Ok(Value::Array(
                items
                    .iter()
                    .map(|item| self.eval_expr(item))
                    .collect::<Result<Vec<_>>>()?,
            )),
            Expression::Vec(items, _) => Ok(Value::Vec(
                items
                    .iter()
                    .map(|item| self.eval_expr(item))
                    .collect::<Result<Vec<_>>>()?,
            )),
            Expression::Unit(_) => Ok(Value::Unit),
            Expression::Var(name, _) => Ok(self.lookup_value(name)?.clone()),
            Expression::Unary { op, expr, .. } => {
                let value = self.eval_expr(expr)?;
                match (op, value) {
                    (UnaryOp::Neg, Value::Int(v)) => Ok(Value::Int(-v)),
                    (UnaryOp::Neg, Value::Float(v)) => {
                        Ok(Value::Float((-f64::from_bits(v)).to_bits()))
                    }
                    (UnaryOp::Not, Value::Bool(v)) => Ok(Value::Bool(!v)),
                    _ => Err(MiniError::runtime("invalid unary operation")),
                }
            }
            Expression::Binary {
                op, left, right, ..
            } => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                self.eval_binary(*op, l, r)
            }
            Expression::Call { name, args, .. } => {
                let values = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<Vec<_>>>()?;
                self.call_function(name, values)
            }
            Expression::MethodCall {
                receiver,
                name,
                args,
                ..
            } => {
                if name == "len" && args.is_empty() {
                    return match self.eval_expr(receiver)? {
                        Value::String(text) => Ok(Value::Int(text.chars().count() as i64)),
                        Value::Array(items) | Value::Vec(items) => {
                            Ok(Value::Int(items.len() as i64))
                        }
                        _ => Err(MiniError::runtime(
                            "method `len` expects String, array, or vec",
                        )),
                    };
                }
                if name == "trim" && args.is_empty() {
                    return match self.eval_expr(receiver)? {
                        Value::String(text) => Ok(Value::String(text.trim().to_string())),
                        _ => Err(MiniError::runtime("method `trim` expects String")),
                    };
                }
                if name == "to_lowercase" && args.is_empty() {
                    return match self.eval_expr(receiver)? {
                        Value::String(text) => Ok(Value::String(text.to_lowercase())),
                        _ => Err(MiniError::runtime("method `to_lowercase` expects String")),
                    };
                }
                if name == "to_uppercase" && args.is_empty() {
                    return match self.eval_expr(receiver)? {
                        Value::String(text) => Ok(Value::String(text.to_uppercase())),
                        _ => Err(MiniError::runtime("method `to_uppercase` expects String")),
                    };
                }
                if matches!(name.as_str(), "contains" | "starts_with" | "ends_with") {
                    if args.len() != 1 {
                        return Err(MiniError::runtime(format!(
                            "method `{}` expects 1 argument",
                            name
                        )));
                    }
                    let Value::String(text) = self.eval_expr(receiver)? else {
                        return Err(MiniError::runtime(format!(
                            "method `{}` expects String receiver",
                            name
                        )));
                    };
                    let Value::String(needle) = self.eval_expr(&args[0])? else {
                        return Err(MiniError::runtime(format!(
                            "method `{}` expects String argument",
                            name
                        )));
                    };
                    return Ok(Value::Bool(match name.as_str() {
                        "contains" => text.contains(&needle),
                        "starts_with" => text.starts_with(&needle),
                        "ends_with" => text.ends_with(&needle),
                        _ => unreachable!(),
                    }));
                }
                if name == "replace" {
                    if args.len() != 2 {
                        return Err(MiniError::runtime("method `replace` expects 2 arguments"));
                    }
                    let Value::String(text) = self.eval_expr(receiver)? else {
                        return Err(MiniError::runtime(
                            "method `replace` expects String receiver",
                        ));
                    };
                    let Value::String(from) = self.eval_expr(&args[0])? else {
                        return Err(MiniError::runtime(
                            "method `replace` expects String pattern",
                        ));
                    };
                    let Value::String(to) = self.eval_expr(&args[1])? else {
                        return Err(MiniError::runtime(
                            "method `replace` expects String replacement",
                        ));
                    };
                    return Ok(Value::String(text.replace(&from, &to)));
                }
                if name == "push" {
                    if args.len() != 1 {
                        return Err(MiniError::runtime("method `push` expects 1 argument"));
                    }
                    let value = self.eval_expr(&args[0])?;
                    let Expression::Var(var_name, _) = &**receiver else {
                        return Err(MiniError::runtime(
                            "method `push` expects variable receiver",
                        ));
                    };
                    let (frame, key) = self.find_var(var_name)?;
                    let var = self.frames[frame].get_mut(&key).unwrap();
                    if !var.mutable {
                        return Err(MiniError::runtime(format!(
                            "cannot call `push` on immutable variable `{}`",
                            var_name
                        )));
                    }
                    let Value::Vec(items) = &mut var.value else {
                        return Err(MiniError::runtime("method `push` expects Vec"));
                    };
                    items.push(value);
                    return Ok(Value::Unit);
                }
                if name == "pop" && args.is_empty() {
                    let Expression::Var(var_name, _) = &**receiver else {
                        return Err(MiniError::runtime("method `pop` expects variable receiver"));
                    };
                    let (frame, key) = self.find_var(var_name)?;
                    let var = self.frames[frame].get_mut(&key).unwrap();
                    if !var.mutable {
                        return Err(MiniError::runtime(format!(
                            "cannot call `pop` on immutable variable `{}`",
                            var_name
                        )));
                    }
                    let Value::Vec(items) = &mut var.value else {
                        return Err(MiniError::runtime("method `pop` expects Vec"));
                    };
                    return Ok(items.pop().map(option_some).unwrap_or_else(option_none));
                }
                if name == "push_str" {
                    if args.len() != 1 {
                        return Err(MiniError::runtime("method `push_str` expects 1 argument"));
                    }
                    let Value::String(extra) = self.eval_expr(&args[0])? else {
                        return Err(MiniError::runtime(
                            "method `push_str` expects String argument",
                        ));
                    };
                    let Expression::Var(var_name, _) = &**receiver else {
                        return Err(MiniError::runtime(
                            "method `push_str` expects variable receiver",
                        ));
                    };
                    let (frame, key) = self.find_var(var_name)?;
                    let var = self.frames[frame].get_mut(&key).unwrap();
                    if !var.mutable {
                        return Err(MiniError::runtime(format!(
                            "cannot call `push_str` on immutable variable `{}`",
                            var_name
                        )));
                    }
                    let Value::String(text) = &mut var.value else {
                        return Err(MiniError::runtime("method `push_str` expects String"));
                    };
                    text.push_str(&extra);
                    return Ok(Value::Unit);
                }
                let receiver_value = if matches!(
                    name.as_str(),
                    "is_some" | "is_none" | "is_ok" | "is_err" | "unwrap_or"
                ) {
                    let value = self.eval_expr(receiver)?;
                    if is_builtin_option_result(&value) {
                        return self.eval_enum_method(value, name, args);
                    }
                    value
                } else {
                    self.eval_expr(receiver)?
                };
                let type_name = match &receiver_value {
                    Value::Struct { name, .. } => name.clone(),
                    Value::Enum { enum_name, .. } => enum_name.clone(),
                    Value::Ref(reference) => match self.frames[reference.frame]
                        .get(&reference.name)
                        .map(|var| &var.value)
                    {
                        Some(Value::Struct { name, .. }) => name.clone(),
                        Some(Value::Enum { enum_name, .. }) => enum_name.clone(),
                        _ => {
                            return Err(MiniError::runtime(
                                "method receiver must be struct or enum",
                            ))
                        }
                    },
                    _ => return Err(MiniError::runtime("method receiver must be struct or enum")),
                };
                let full_name = format!("{}::{}", type_name, name);
                let function = self.functions.get(&full_name).ok_or_else(|| {
                    MiniError::runtime(format!("function `{}` not found", full_name))
                })?;
                let first_ty = function.params.first().map(|param| &param.ty);
                let receiver_arg = if matches!(first_ty, Some(Type::Ref(_)) | Some(Type::MutRef(_)))
                {
                    match &**receiver {
                        Expression::Var(var, _) => {
                            let (frame, _) = self.find_var(var)?;
                            Value::Ref(RefValue {
                                frame,
                                name: var.clone(),
                                mutable: matches!(first_ty, Some(Type::MutRef(_))),
                            })
                        }
                        _ => receiver_value,
                    }
                } else {
                    receiver_value
                };
                let mut values = vec![receiver_arg];
                for arg in args {
                    values.push(self.eval_expr(arg)?);
                }
                self.call_function(&full_name, values)
            }
            Expression::StructLiteral { name, fields, .. } => {
                let mut values = Vec::new();
                for (field, expr) in fields {
                    values.push((field.clone(), self.eval_expr(expr)?));
                }
                Ok(Value::Struct {
                    name: name.clone(),
                    fields: values,
                })
            }
            Expression::EnumLiteral {
                enum_name,
                variant,
                value,
                ..
            } => {
                let value = if let Some(value) = value {
                    Some(Box::new(self.eval_expr(value)?))
                } else {
                    None
                };
                Ok(Value::Enum {
                    enum_name: enum_name.clone(),
                    variant: variant.clone(),
                    value,
                })
            }
            Expression::Index { target, index, .. } => {
                let target = self.eval_expr(target)?;
                let index = self.eval_expr(index)?;
                let Value::Int(index) = index else {
                    return Err(MiniError::runtime("array index must be i64"));
                };
                let items = match target {
                    Value::Array(items) | Value::Vec(items) => items,
                    _ => return Err(MiniError::runtime("cannot index non-array value")),
                };
                items
                    .get(index as usize)
                    .cloned()
                    .ok_or_else(|| MiniError::runtime("array index out of bounds"))
            }
            Expression::Field { target, field, .. } => {
                let target = self.eval_expr(target)?;
                match target {
                    Value::Ref(reference) => {
                        let value = self.frames[reference.frame]
                            .get(&reference.name)
                            .ok_or_else(|| {
                                MiniError::runtime(format!(
                                    "dangling reference `{}`",
                                    reference.name
                                ))
                            })?
                            .value
                            .clone();
                        self.field_value(value, field)
                    }
                    Value::Tuple(items) => {
                        let index = field
                            .parse::<usize>()
                            .map_err(|_| MiniError::runtime("tuple field must be numeric"))?;
                        items
                            .get(index)
                            .cloned()
                            .ok_or_else(|| MiniError::runtime("tuple field out of bounds"))
                    }
                    Value::Struct { name, fields } => fields
                        .into_iter()
                        .find(|(candidate, _)| candidate == field)
                        .map(|(_, value)| value)
                        .ok_or_else(|| {
                            MiniError::runtime(format!(
                                "unknown field `{}` for struct `{}`",
                                field, name
                            ))
                        }),
                    _ => Err(MiniError::runtime("cannot access field on value")),
                }
            }
            Expression::Match { value, arms, .. } => {
                let value = self.eval_expr(value)?;
                let Value::Enum {
                    enum_name,
                    variant,
                    value,
                } = value
                else {
                    return Err(MiniError::runtime("match expects enum value"));
                };
                for arm in arms {
                    self.frames.push(HashMap::new());
                    let matched =
                        self.pattern_matches(&arm.pattern, &enum_name, &variant, value.as_deref())?;
                    if matched {
                        let result = self.eval_expr(&arm.body);
                        self.frames.pop();
                        return result;
                    }
                    self.frames.pop();
                }
                Err(MiniError::runtime("non-exhaustive match at runtime"))
            }
            Expression::Block(block) => match self.eval_block(block)? {
                Flow::Value(v) | Flow::Return(v) => Ok(v),
                Flow::Break | Flow::Continue => {
                    Err(MiniError::runtime("loop control escaped block expression"))
                }
            },
            Expression::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                if self.eval_expr(condition)? == Value::Bool(true) {
                    match self.eval_block(then_block)? {
                        Flow::Value(v) | Flow::Return(v) => Ok(v),
                        Flow::Break | Flow::Continue => {
                            Err(MiniError::runtime("loop control escaped if expression"))
                        }
                    }
                } else if let Some(block) = else_block {
                    match self.eval_block(block)? {
                        Flow::Value(v) | Flow::Return(v) => Ok(v),
                        Flow::Break | Flow::Continue => {
                            Err(MiniError::runtime("loop control escaped if expression"))
                        }
                    }
                } else {
                    Ok(Value::Unit)
                }
            }
            Expression::Ref { mutable, expr, .. } => {
                let Expression::Var(name, _) = &**expr else {
                    return Err(MiniError::runtime("reference target must be variable"));
                };
                let (frame, _) = self.find_var(name)?;
                Ok(Value::Ref(RefValue {
                    frame,
                    name: name.clone(),
                    mutable: *mutable,
                }))
            }
            Expression::Deref { expr, .. } => {
                let value = self.eval_expr(expr)?;
                let Value::Ref(r) = value else {
                    return Err(MiniError::runtime("cannot dereference non-reference value"));
                };
                Ok(self.frames[r.frame]
                    .get(&r.name)
                    .ok_or_else(|| MiniError::runtime(format!("dangling reference `{}`", r.name)))?
                    .value
                    .clone())
            }
        }
    }

    fn eval_expr_statement(&mut self, expr: &Expression) -> Result<Flow> {
        match expr {
            Expression::Block(block) => self.eval_block(block),
            Expression::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                if self.eval_expr(condition)? == Value::Bool(true) {
                    self.eval_block(then_block)
                } else if let Some(block) = else_block {
                    self.eval_block(block)
                } else {
                    Ok(Flow::Value(Value::Unit))
                }
            }
            _ => Ok(Flow::Value(self.eval_expr(expr)?)),
        }
    }

    fn assign_target(&mut self, target: &Expression, value: Value) -> Result<()> {
        match target {
            Expression::Var(name, _) => {
                let (frame, key) = self.find_var(name)?;
                let var = self.frames[frame].get_mut(&key).unwrap();
                if !var.mutable {
                    return Err(MiniError::runtime(format!(
                        "cannot assign to immutable variable `{}`",
                        name
                    )));
                }
                var.value = value;
                Ok(())
            }
            Expression::Deref { expr, .. } => {
                let ref_value = self.eval_expr(expr)?;
                let Value::Ref(r) = ref_value else {
                    return Err(MiniError::runtime(
                        "cannot assign through non-reference value",
                    ));
                };
                if !r.mutable {
                    return Err(MiniError::runtime(
                        "cannot assign through immutable reference",
                    ));
                }
                let var = self.frames[r.frame].get_mut(&r.name).ok_or_else(|| {
                    MiniError::runtime(format!("dangling reference `{}`", r.name))
                })?;
                var.value = value;
                Ok(())
            }
            Expression::Index { target, index, .. } => {
                let index = self.eval_expr(index)?;
                let Value::Int(index) = index else {
                    return Err(MiniError::runtime("array index must be i64"));
                };
                self.assign_nested(target, PathStep::Index(index as usize), value)
            }
            Expression::Field { target, field, .. } => {
                self.assign_nested(target, PathStep::Field(field.clone()), value)
            }
            _ => Err(MiniError::runtime("invalid assignment target")),
        }
    }

    fn pattern_matches(
        &mut self,
        pattern: &Pattern,
        enum_name: &str,
        variant: &str,
        payload: Option<&Value>,
    ) -> Result<bool> {
        match pattern {
            Pattern::Wildcard => Ok(true),
            Pattern::EnumVariant {
                enum_name: pat_enum,
                variant: pat_variant,
                binding,
            } => {
                if pat_enum != enum_name || pat_variant != variant {
                    return Ok(false);
                }
                if let Some(binding) = binding {
                    let Some(value) = payload else {
                        return Err(MiniError::runtime("enum pattern expected payload"));
                    };
                    self.frames.last_mut().unwrap().insert(
                        binding.clone(),
                        RuntimeVar {
                            value: value.clone(),
                            mutable: false,
                        },
                    );
                }
                Ok(true)
            }
        }
    }

    fn assign_nested(&mut self, target: &Expression, step: PathStep, value: Value) -> Result<()> {
        match target {
            Expression::Var(name, _) if name == "self" => {
                let self_value = self.lookup_value(name)?.clone();
                if let Value::Ref(reference) = self_value {
                    if !reference.mutable {
                        return Err(MiniError::runtime(
                            "cannot assign through immutable self reference",
                        ));
                    }
                    let var = self.frames[reference.frame]
                        .get_mut(&reference.name)
                        .ok_or_else(|| {
                            MiniError::runtime(format!("dangling reference `{}`", reference.name))
                        })?;
                    return assign_value_step(&mut var.value, step, value);
                }
                let (frame, key) = self.find_var(name)?;
                let var = self.frames[frame].get_mut(&key).unwrap();
                assign_value_step(&mut var.value, step, value)
            }
            Expression::Var(name, _) => {
                let (frame, key) = self.find_var(name)?;
                let var = self.frames[frame].get_mut(&key).unwrap();
                if !var.mutable {
                    return Err(MiniError::runtime(format!(
                        "cannot assign through immutable variable `{}`",
                        name
                    )));
                }
                assign_value_step(&mut var.value, step, value)
            }
            Expression::Index { target, index, .. } => {
                let index = self.eval_expr(index)?;
                let Value::Int(index) = index else {
                    return Err(MiniError::runtime("array index must be i64"));
                };
                let mut path = vec![PathStep::Index(index as usize), step];
                self.assign_path(target, &mut path, value)
            }
            Expression::Field { target, field, .. } => {
                let mut path = vec![PathStep::Field(field.clone()), step];
                self.assign_path(target, &mut path, value)
            }
            _ => Err(MiniError::runtime("invalid nested assignment target")),
        }
    }

    fn assign_path(
        &mut self,
        target: &Expression,
        path: &mut Vec<PathStep>,
        value: Value,
    ) -> Result<()> {
        match target {
            Expression::Var(name, _) => {
                let (frame, key) = self.find_var(name)?;
                let var = self.frames[frame].get_mut(&key).unwrap();
                if !var.mutable {
                    return Err(MiniError::runtime(format!(
                        "cannot assign through immutable variable `{}`",
                        name
                    )));
                }
                assign_value_path(&mut var.value, path, value)
            }
            Expression::Index { target, index, .. } => {
                let index = self.eval_expr(index)?;
                let Value::Int(index) = index else {
                    return Err(MiniError::runtime("array index must be i64"));
                };
                path.insert(0, PathStep::Index(index as usize));
                self.assign_path(target, path, value)
            }
            Expression::Field { target, field, .. } => {
                path.insert(0, PathStep::Field(field.clone()));
                self.assign_path(target, path, value)
            }
            _ => Err(MiniError::runtime("invalid nested assignment target")),
        }
    }

    fn eval_enum_method(
        &mut self,
        receiver: Value,
        name: &str,
        args: &[Expression],
    ) -> Result<Value> {
        match name {
            "is_some" => {
                if !args.is_empty() {
                    return Err(MiniError::runtime("method `is_some` expects 0 arguments"));
                }
                Ok(Value::Bool(matches!(
                    receiver,
                    Value::Enum {
                        enum_name,
                        variant,
                        ..
                    } if enum_name == "Option" && variant == "Some"
                )))
            }
            "is_none" => {
                if !args.is_empty() {
                    return Err(MiniError::runtime("method `is_none` expects 0 arguments"));
                }
                Ok(Value::Bool(matches!(
                    receiver,
                    Value::Enum {
                        enum_name,
                        variant,
                        ..
                    } if enum_name == "Option" && variant == "None"
                )))
            }
            "is_ok" => {
                if !args.is_empty() {
                    return Err(MiniError::runtime("method `is_ok` expects 0 arguments"));
                }
                Ok(Value::Bool(matches!(
                    receiver,
                    Value::Enum {
                        enum_name,
                        variant,
                        ..
                    } if enum_name == "Result" && variant == "Ok"
                )))
            }
            "is_err" => {
                if !args.is_empty() {
                    return Err(MiniError::runtime("method `is_err` expects 0 arguments"));
                }
                Ok(Value::Bool(matches!(
                    receiver,
                    Value::Enum {
                        enum_name,
                        variant,
                        ..
                    } if enum_name == "Result" && variant == "Err"
                )))
            }
            "unwrap_or" => {
                if args.len() != 1 {
                    return Err(MiniError::runtime("method `unwrap_or` expects 1 argument"));
                }
                match receiver {
                    Value::Enum {
                        enum_name,
                        variant,
                        value,
                    } if (enum_name == "Option" && variant == "Some")
                        || (enum_name == "Result" && variant == "Ok") =>
                    {
                        Ok(value.map(|v| *v).unwrap_or(Value::Unit))
                    }
                    Value::Enum { .. } => self.eval_expr(&args[0]),
                    _ => Err(MiniError::runtime(
                        "method `unwrap_or` expects Option or Result",
                    )),
                }
            }
            _ => Err(MiniError::runtime(format!("unknown method `{}`", name))),
        }
    }

    fn eval_binary(&self, op: BinaryOp, l: Value, r: Value) -> Result<Value> {
        match (op, l, r) {
            (BinaryOp::Add, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (BinaryOp::Sub, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (BinaryOp::Mul, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (BinaryOp::Div, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
            (BinaryOp::Rem, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
            (BinaryOp::Add, Value::Float(a), Value::Float(b)) => {
                Ok(float_value(f64::from_bits(a) + f64::from_bits(b)))
            }
            (BinaryOp::Sub, Value::Float(a), Value::Float(b)) => {
                Ok(float_value(f64::from_bits(a) - f64::from_bits(b)))
            }
            (BinaryOp::Mul, Value::Float(a), Value::Float(b)) => {
                Ok(float_value(f64::from_bits(a) * f64::from_bits(b)))
            }
            (BinaryOp::Div, Value::Float(a), Value::Float(b)) => {
                Ok(float_value(f64::from_bits(a) / f64::from_bits(b)))
            }
            (BinaryOp::Rem, Value::Float(a), Value::Float(b)) => {
                Ok(float_value(f64::from_bits(a) % f64::from_bits(b)))
            }
            (BinaryOp::Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            (BinaryOp::Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            (BinaryOp::Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            (BinaryOp::Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
            (BinaryOp::Lt, Value::Float(a), Value::Float(b)) => {
                Ok(Value::Bool(f64::from_bits(a) < f64::from_bits(b)))
            }
            (BinaryOp::Le, Value::Float(a), Value::Float(b)) => {
                Ok(Value::Bool(f64::from_bits(a) <= f64::from_bits(b)))
            }
            (BinaryOp::Gt, Value::Float(a), Value::Float(b)) => {
                Ok(Value::Bool(f64::from_bits(a) > f64::from_bits(b)))
            }
            (BinaryOp::Ge, Value::Float(a), Value::Float(b)) => {
                Ok(Value::Bool(f64::from_bits(a) >= f64::from_bits(b)))
            }
            (BinaryOp::Eq, a, b) => Ok(Value::Bool(a == b)),
            (BinaryOp::Ne, a, b) => Ok(Value::Bool(a != b)),
            (BinaryOp::And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a && b)),
            (BinaryOp::Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a || b)),
            _ => Err(MiniError::runtime("invalid binary operation")),
        }
    }

    fn field_value(&self, target: Value, field: &str) -> Result<Value> {
        match target {
            Value::Tuple(items) => {
                let index = field
                    .parse::<usize>()
                    .map_err(|_| MiniError::runtime("tuple field must be numeric"))?;
                items
                    .get(index)
                    .cloned()
                    .ok_or_else(|| MiniError::runtime("tuple field out of bounds"))
            }
            Value::Struct { name, fields } => fields
                .into_iter()
                .find(|(candidate, _)| candidate == field)
                .map(|(_, value)| value)
                .ok_or_else(|| {
                    MiniError::runtime(format!("unknown field `{}` for struct `{}`", field, name))
                }),
            _ => Err(MiniError::runtime("cannot access field on value")),
        }
    }

    fn lookup_value(&self, name: &str) -> Result<&Value> {
        let (frame, key) = self.find_var(name)?;
        Ok(&self.frames[frame].get(&key).unwrap().value)
    }

    fn find_var(&self, name: &str) -> Result<(usize, String)> {
        for (idx, frame) in self.frames.iter().enumerate().rev() {
            if frame.contains_key(name) {
                return Ok((idx, name.to_string()));
            }
        }
        Err(MiniError::runtime(format!("variable `{}` not found", name)))
    }
}

#[derive(Clone)]
enum PathStep {
    Index(usize),
    Field(String),
}

fn float_value(value: f64) -> Value {
    Value::Float(value.to_bits())
}

fn format_macro_values(args: &[Value]) -> Result<String> {
    let Some(Value::String(template)) = args.first() else {
        return Err(MiniError::runtime(
            "formatting macro expects a String format literal",
        ));
    };
    let values = args
        .iter()
        .skip(1)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut value_index = 0usize;
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                out.push('{');
            } else if chars.peek() == Some(&'}') {
                chars.next();
                let value = values.get(value_index).ok_or_else(|| {
                    MiniError::runtime("formatting macro has more `{}` slots than values")
                })?;
                out.push_str(value);
                value_index += 1;
            } else {
                return Err(MiniError::runtime(
                    "formatting macro only supports `{}` placeholders",
                ));
            }
        } else if ch == '}' {
            if chars.peek() == Some(&'}') {
                chars.next();
                out.push('}');
            } else {
                return Err(MiniError::runtime("unmatched `}` in format string"));
            }
        } else {
            out.push(ch);
        }
    }
    if value_index != values.len() {
        return Err(MiniError::runtime(
            "formatting macro has more values than `{}` slots",
        ));
    }
    Ok(out)
}

fn builtin_alias(name: &str) -> &str {
    match name {
        "std::io::read_line" => "input",
        "std::io::read_key" => "read_key",
        "std::io::read_i64" => "read_i64_alias",
        "std::io::read_f64" => "read_f64_alias",
        "io::read_line" => "input",
        "io::read_key" => "read_key",
        "io::read_i64" => "read_i64_alias",
        "io::read_f64" => "read_f64_alias",
        "std::fs::read_to_string" => "read_file",
        "std::fs::write" => "write_file",
        "fs::read_to_string" => "read_file",
        "fs::write" => "write_file",
        "std::time::sleep_ms" => "sleep_ms",
        "std::time::clock_ms" => "clock_ms",
        "time::sleep_ms" => "sleep_ms",
        "time::clock_ms" => "clock_ms",
        "game::clear" => "clear",
        "game::color" => "color",
        "game::rand_i64" => "rand_i64",
        "game::read_key" => "read_key",
        "game::sleep_ms" => "sleep_ms",
        other => other,
    }
}

fn read_key_nonblocking() -> String {
    #[cfg(windows)]
    unsafe {
        if _kbhit() == 0 {
            return String::new();
        }
        let code = _getch();
        if code == 0 || code == 224 {
            if _kbhit() == 0 {
                return String::new();
            }
            return match _getch() {
                72 => "up".to_string(),
                80 => "down".to_string(),
                75 => "left".to_string(),
                77 => "right".to_string(),
                _ => String::new(),
            };
        }
        char::from_u32(code as u32)
            .map(|ch| ch.to_ascii_lowercase().to_string())
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

fn result_ok(value: Value) -> Value {
    Value::Enum {
        enum_name: "Result".to_string(),
        variant: "Ok".to_string(),
        value: Some(Box::new(value)),
    }
}

fn result_err(message: String) -> Value {
    Value::Enum {
        enum_name: "Result".to_string(),
        variant: "Err".to_string(),
        value: Some(Box::new(Value::String(message))),
    }
}

fn option_some(value: Value) -> Value {
    Value::Enum {
        enum_name: "Option".to_string(),
        variant: "Some".to_string(),
        value: Some(Box::new(value)),
    }
}

fn option_none() -> Value {
    Value::Enum {
        enum_name: "Option".to_string(),
        variant: "None".to_string(),
        value: None,
    }
}

fn is_builtin_option_result(value: &Value) -> bool {
    matches!(
        value,
        Value::Enum { enum_name, .. } if enum_name == "Option" || enum_name == "Result"
    )
}

fn assign_value_step(target: &mut Value, step: PathStep, value: Value) -> Result<()> {
    match step {
        PathStep::Index(index) => {
            let items = match target {
                Value::Array(items) | Value::Vec(items) => items,
                _ => return Err(MiniError::runtime("cannot index-assign non-array value")),
            };
            let Some(slot) = items.get_mut(index) else {
                return Err(MiniError::runtime("array index out of bounds"));
            };
            *slot = value;
            Ok(())
        }
        PathStep::Field(field) => match target {
            Value::Tuple(items) => {
                let index = field
                    .parse::<usize>()
                    .map_err(|_| MiniError::runtime("tuple field must be numeric"))?;
                let Some(slot) = items.get_mut(index) else {
                    return Err(MiniError::runtime("tuple field out of bounds"));
                };
                *slot = value;
                Ok(())
            }
            Value::Struct { name, fields } => {
                let Some((_, slot)) = fields.iter_mut().find(|(candidate, _)| candidate == &field)
                else {
                    return Err(MiniError::runtime(format!(
                        "unknown field `{}` for struct `{}`",
                        field, name
                    )));
                };
                *slot = value;
                Ok(())
            }
            _ => Err(MiniError::runtime("cannot field-assign value")),
        },
    }
}

fn assign_value_path(target: &mut Value, path: &mut Vec<PathStep>, value: Value) -> Result<()> {
    if path.len() == 1 {
        return assign_value_step(target, path.remove(0), value);
    }
    let step = path.remove(0);
    match step {
        PathStep::Index(index) => {
            let items = match target {
                Value::Array(items) | Value::Vec(items) => items,
                _ => return Err(MiniError::runtime("cannot index-assign non-array value")),
            };
            let Some(next) = items.get_mut(index) else {
                return Err(MiniError::runtime("array index out of bounds"));
            };
            assign_value_path(next, path, value)
        }
        PathStep::Field(field) => match target {
            Value::Tuple(items) => {
                let index = field
                    .parse::<usize>()
                    .map_err(|_| MiniError::runtime("tuple field must be numeric"))?;
                let Some(next) = items.get_mut(index) else {
                    return Err(MiniError::runtime("tuple field out of bounds"));
                };
                assign_value_path(next, path, value)
            }
            Value::Struct { name, fields } => {
                let Some((_, next)) = fields.iter_mut().find(|(candidate, _)| candidate == &field)
                else {
                    return Err(MiniError::runtime(format!(
                        "unknown field `{}` for struct `{}`",
                        field, name
                    )));
                };
                assign_value_path(next, path, value)
            }
            _ => Err(MiniError::runtime("cannot field-assign value")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{check_program, parse_source};

    #[test]
    fn runs_mut_ref_program() {
        let p = parse_source("fn main(){ let mut hp:i64=10; damage(&mut hp); print(hp); } fn damage(v:&mut i64){ *v = *v - 1; }").unwrap();
        check_program(&p).unwrap();
        let out = Interpreter::new(&p).run().unwrap();
        assert_eq!(out, vec!["9"]);
    }
}
