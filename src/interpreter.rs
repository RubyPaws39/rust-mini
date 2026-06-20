use crate::ast::*;
use crate::error::{MiniError, Result, Span};
use crate::value::{RefValue, Value};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::f64::consts::PI;
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

#[derive(Debug, Clone)]
struct LogoLine {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    color: String,
    width: i64,
}

#[derive(Debug, Clone)]
struct LogoCircle {
    x: f64,
    y: f64,
    radius: f64,
    color: String,
    width: i64,
}

#[derive(Debug, Clone)]
struct LogoState {
    x: f64,
    y: f64,
    heading_degrees: f64,
    pen_down: bool,
    color: String,
    width: i64,
    background: String,
    lines: Vec<LogoLine>,
    circles: Vec<LogoCircle>,
}

impl Default for LogoState {
    fn default() -> Self {
        Self {
            x: 250.0,
            y: 250.0,
            heading_degrees: 0.0,
            pen_down: true,
            color: "black".to_string(),
            width: 3,
            background: "white".to_string(),
            lines: Vec::new(),
            circles: Vec::new(),
        }
    }
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
    logo: LogoState,
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
            logo: LogoState::default(),
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
        if let Some(value) = self.call_logo_function(name, &args)? {
            return Ok(value);
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

    fn call_logo_function(&mut self, name: &str, args: &[Value]) -> Result<Option<Value>> {
        match name {
            "logo_forward" => {
                let distance = expect_logo_i64(name, args, 1)? as f64;
                self.logo_move(distance);
                Ok(Some(Value::Unit))
            }
            "logo_back" => {
                let distance = expect_logo_i64(name, args, 1)? as f64;
                self.logo_move(-distance);
                Ok(Some(Value::Unit))
            }
            "logo_right" => {
                let degrees = expect_logo_i64(name, args, 1)? as f64;
                self.logo.heading_degrees =
                    normalize_logo_heading(self.logo.heading_degrees + degrees);
                Ok(Some(Value::Unit))
            }
            "logo_left" => {
                let degrees = expect_logo_i64(name, args, 1)? as f64;
                self.logo.heading_degrees =
                    normalize_logo_heading(self.logo.heading_degrees - degrees);
                Ok(Some(Value::Unit))
            }
            "logo_set_position" => {
                expect_logo_arg_count(name, args, 2)?;
                let x = expect_logo_i64_at(name, args, 0)? as f64;
                let y = expect_logo_i64_at(name, args, 1)? as f64;
                self.logo_move_to(x, y);
                Ok(Some(Value::Unit))
            }
            "logo_home" => {
                expect_logo_arg_count(name, args, 0)?;
                self.logo_move_to(250.0, 250.0);
                self.logo.heading_degrees = 0.0;
                Ok(Some(Value::Unit))
            }
            "logo_heading" => {
                expect_logo_arg_count(name, args, 0)?;
                Ok(Some(Value::Int(self.logo.heading_degrees.round() as i64)))
            }
            "logo_set_heading" => {
                let degrees = expect_logo_i64(name, args, 1)? as f64;
                self.logo.heading_degrees = normalize_logo_heading(degrees);
                Ok(Some(Value::Unit))
            }
            "logo_circle" => {
                let radius = expect_logo_i64(name, args, 1)? as f64;
                if self.logo.pen_down {
                    self.logo.circles.push(LogoCircle {
                        x: self.logo.x,
                        y: self.logo.y,
                        radius,
                        color: self.logo.color.clone(),
                        width: self.logo.width,
                    });
                }
                Ok(Some(Value::Unit))
            }
            "logo_width" => {
                let width = expect_logo_i64(name, args, 1)?;
                if width <= 0 {
                    return Err(MiniError::runtime(
                        "function `logo_width` expects positive i64",
                    ));
                }
                self.logo.width = width;
                Ok(Some(Value::Unit))
            }
            "logo_background" => {
                let color = expect_logo_string(name, args, 1)?;
                self.logo.background = color.to_string();
                Ok(Some(Value::Unit))
            }
            "logo_pen_up" => {
                expect_logo_arg_count(name, args, 0)?;
                self.logo.pen_down = false;
                Ok(Some(Value::Unit))
            }
            "logo_pen_down" => {
                expect_logo_arg_count(name, args, 0)?;
                self.logo.pen_down = true;
                Ok(Some(Value::Unit))
            }
            "logo_pen_color" => {
                let color = expect_logo_string(name, args, 1)?;
                self.logo.color = color.to_string();
                Ok(Some(Value::Unit))
            }
            "logo_clear" => {
                expect_logo_arg_count(name, args, 0)?;
                self.logo = LogoState::default();
                Ok(Some(Value::Unit))
            }
            "logo_save" => {
                let path = expect_logo_string(name, args, 1)?;
                fs::write(path, logo_svg(&self.logo, 500, 500)).map_err(|e| {
                    MiniError::runtime(format!("failed to write SVG `{}`: {}", path, e))
                })?;
                Ok(Some(Value::Unit))
            }
            "logo_save_with_size" => {
                expect_logo_arg_count(name, args, 3)?;
                let path = expect_logo_string(name, args, 3)?;
                let width = expect_logo_i64_at(name, args, 1)?;
                let height = expect_logo_i64_at(name, args, 2)?;
                if width <= 0 || height <= 0 {
                    return Err(MiniError::runtime(
                        "function `logo_save_with_size` expects positive width and height",
                    ));
                }
                fs::write(path, logo_svg(&self.logo, width, height)).map_err(|e| {
                    MiniError::runtime(format!("failed to write SVG `{}`: {}", path, e))
                })?;
                Ok(Some(Value::Unit))
            }
            _ => Ok(None),
        }
    }

    fn logo_move_to(&mut self, x: f64, y: f64) {
        let start_x = self.logo.x;
        let start_y = self.logo.y;
        self.logo.x = x;
        self.logo.y = y;
        if self.logo.pen_down {
            self.logo.lines.push(LogoLine {
                x1: start_x,
                y1: start_y,
                x2: self.logo.x,
                y2: self.logo.y,
                color: self.logo.color.clone(),
                width: self.logo.width,
            });
        }
    }

    fn logo_move(&mut self, distance: f64) {
        let start_x = self.logo.x;
        let start_y = self.logo.y;
        let radians = self.logo.heading_degrees * PI / 180.0;
        self.logo.x += distance * radians.cos();
        self.logo.y += distance * radians.sin();
        if self.logo.pen_down {
            self.logo.lines.push(LogoLine {
                x1: start_x,
                y1: start_y,
                x2: self.logo.x,
                y2: self.logo.y,
                color: self.logo.color.clone(),
                width: self.logo.width,
            });
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
                _ => self.eval_expr_with_try(tail)?,
            }
        } else {
            Flow::Value(Value::Unit)
        };
        self.frames.pop();
        Ok(flow)
    }

    fn bind_let_pattern(
        &mut self,
        pattern: &LetPattern,
        value: Value,
        mutable: bool,
        span: Span,
    ) -> Result<()> {
        match pattern {
            LetPattern::Ident(name) => {
                self.frames
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), RuntimeVar { value, mutable });
                Ok(())
            }
            LetPattern::Wildcard => Ok(()),
            LetPattern::Unit => {
                if value == Value::Unit {
                    Ok(())
                } else {
                    Err(MiniError::runtime(format!(
                        "unit pattern expects unit value, found `{}`",
                        value
                    )))
                }
            }
            LetPattern::Tuple(patterns) => {
                let Value::Tuple(values) = value else {
                    return Err(MiniError::runtime("tuple pattern expects tuple value"));
                };
                if patterns.len() != values.len() {
                    return Err(MiniError::runtime(format!(
                        "tuple pattern has {} fields, value has {}",
                        patterns.len(),
                        values.len()
                    )));
                }
                for (pattern, value) in patterns.iter().zip(values) {
                    self.bind_let_pattern(pattern, value, mutable, span)?;
                }
                Ok(())
            }
        }
    }
    fn eval_statement(&mut self, stmt: &Statement) -> Result<Flow> {
        match stmt {
            Statement::Let {
                pattern,
                mutable,
                value,
                span,
                ..
            } => {
                let value = match self.eval_expr_with_try(value)? {
                    Flow::Value(value) => value,
                    flow @ Flow::Return(_) => return Ok(flow),
                    Flow::Break | Flow::Continue => {
                        return Err(MiniError::runtime("loop control escaped let expression"))
                    }
                };
                self.bind_let_pattern(pattern, value, *mutable, *span)?;
                Ok(Flow::Value(Value::Unit))
            }
            Statement::Assign { target, value, .. } => {
                let value = match self.eval_expr_with_try(value)? {
                    Flow::Value(value) => value,
                    flow @ Flow::Return(_) => return Ok(flow),
                    Flow::Break | Flow::Continue => {
                        return Err(MiniError::runtime("loop control escaped assignment"))
                    }
                };
                self.assign_target(target, value)?;
                Ok(Flow::Value(Value::Unit))
            }
            Statement::Expr(expr) => self.eval_expr_statement(expr),
            Statement::Return { value, .. } => {
                let value = if let Some(value) = value {
                    match self.eval_expr_with_try(value)? {
                        Flow::Value(value) => value,
                        flow @ Flow::Return(_) => return Ok(flow),
                        Flow::Break | Flow::Continue => {
                            return Err(MiniError::runtime("loop control escaped return"))
                        }
                    }
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
                loop {
                    let cond = match self.eval_expr_with_try(condition)? {
                        Flow::Value(value) => value,
                        flow @ Flow::Return(_) => return Ok(flow),
                        Flow::Break | Flow::Continue => {
                            return Err(MiniError::runtime("loop control escaped condition"))
                        }
                    };
                    if cond != Value::Bool(true) {
                        break;
                    }
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
                let iterable = match self.eval_expr_with_try(iterable)? {
                    Flow::Value(value) => value,
                    flow @ Flow::Return(_) => return Ok(flow),
                    Flow::Break | Flow::Continue => {
                        return Err(MiniError::runtime("loop control escaped iterable"))
                    }
                };
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
                for arm in arms {
                    self.frames.push(HashMap::new());
                    let matched = self.pattern_matches(&arm.pattern, &value)?;
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
            Expression::Try { expr, .. } => {
                let value = match self.eval_expr_with_try(expr)? {
                    Flow::Value(value) => value,
                    Flow::Return(_) => return Err(MiniError::runtime(
                        "`?` early return is only supported in statement and block-tail positions",
                    )),
                    Flow::Break | Flow::Continue => {
                        return Err(MiniError::runtime("loop control escaped `?` expression"))
                    }
                };
                match self.apply_try(value)? {
                    Flow::Value(value) => Ok(value),
                    Flow::Return(_) => Err(MiniError::runtime(
                        "`?` early return is only supported in statement and block-tail positions",
                    )),
                    Flow::Break | Flow::Continue => {
                        Err(MiniError::runtime("loop control escaped `?` expression"))
                    }
                }
            }
        }
    }

    fn eval_expr_with_try(&mut self, expr: &Expression) -> Result<Flow> {
        match expr {
            Expression::Try { expr, .. } => {
                let value = match self.eval_expr_with_try(expr)? {
                    Flow::Value(value) => value,
                    flow @ (Flow::Return(_) | Flow::Break | Flow::Continue) => return Ok(flow),
                };
                self.apply_try(value)
            }
            _ => Ok(Flow::Value(self.eval_expr(expr)?)),
        }
    }

    fn apply_try(&mut self, value: Value) -> Result<Flow> {
        match value {
            Value::Enum {
                enum_name,
                variant,
                value,
            } if enum_name == "Result" && variant == "Ok" => {
                let Some(value) = value else {
                    return Err(MiniError::runtime("Result::Ok used with `?` needs payload"));
                };
                Ok(Flow::Value(*value))
            }
            Value::Enum {
                enum_name,
                variant,
                value,
            } if enum_name == "Result" && variant == "Err" => Ok(Flow::Return(Value::Enum {
                enum_name,
                variant,
                value,
            })),
            Value::Enum {
                enum_name,
                variant,
                value,
            } if enum_name == "Option" && variant == "Some" => {
                let Some(value) = value else {
                    return Err(MiniError::runtime(
                        "Option::Some used with `?` needs payload",
                    ));
                };
                Ok(Flow::Value(*value))
            }
            Value::Enum {
                enum_name,
                variant,
                value,
            } if enum_name == "Option" && variant == "None" => Ok(Flow::Return(Value::Enum {
                enum_name,
                variant,
                value,
            })),
            other => Err(MiniError::runtime(format!(
                "`?` expects Option or Result, found `{}`",
                other
            ))),
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
                let condition = match self.eval_expr_with_try(condition)? {
                    Flow::Value(value) => value,
                    flow @ Flow::Return(_) => return Ok(flow),
                    Flow::Break | Flow::Continue => {
                        return Err(MiniError::runtime("loop control escaped if condition"))
                    }
                };
                if condition == Value::Bool(true) {
                    self.eval_block(then_block)
                } else if let Some(block) = else_block {
                    self.eval_block(block)
                } else {
                    Ok(Flow::Value(Value::Unit))
                }
            }
            _ => self.eval_expr_with_try(expr),
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

    fn pattern_matches(&mut self, pattern: &Pattern, value: &Value) -> Result<bool> {
        match pattern {
            Pattern::Wildcard => Ok(true),
            Pattern::Binding(name) => {
                self.frames.last_mut().unwrap().insert(
                    name.clone(),
                    RuntimeVar {
                        value: value.clone(),
                        mutable: false,
                    },
                );
                Ok(true)
            }
            Pattern::Int(expected) => Ok(matches!(value, Value::Int(actual) if actual == expected)),
            Pattern::Bool(expected) => {
                Ok(matches!(value, Value::Bool(actual) if actual == expected))
            }
            Pattern::String(expected) => {
                Ok(matches!(value, Value::String(actual) if actual == expected))
            }
            Pattern::Unit => Ok(*value == Value::Unit),
            Pattern::Tuple(patterns) => {
                let Value::Tuple(values) = value else {
                    return Ok(false);
                };
                if patterns.len() != values.len() {
                    return Ok(false);
                }
                for (pattern, value) in patterns.iter().zip(values) {
                    if !self.pattern_matches(pattern, value)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Pattern::EnumVariant {
                enum_name: pat_enum,
                variant: pat_variant,
                binding,
            } => {
                let Value::Enum {
                    enum_name,
                    variant,
                    value,
                } = value
                else {
                    return Ok(false);
                };
                if pat_enum != enum_name || pat_variant != variant {
                    return Ok(false);
                }
                if let Some(binding) = binding {
                    let Some(value) = value else {
                        return Err(MiniError::runtime("enum pattern expected payload"));
                    };
                    self.frames.last_mut().unwrap().insert(
                        binding.clone(),
                        RuntimeVar {
                            value: (**value).clone(),
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

fn expect_logo_arg_count(name: &str, args: &[Value], expected: usize) -> Result<()> {
    if args.len() != expected {
        return Err(MiniError::runtime(format!(
            "function `{}` expects {} argument{}",
            name,
            expected,
            if expected == 1 { "" } else { "s" }
        )));
    }
    Ok(())
}

fn expect_logo_i64<'a>(name: &str, args: &'a [Value], expected: usize) -> Result<i64> {
    expect_logo_arg_count(name, args, expected)?;
    expect_logo_i64_at(name, args, 0)
}

fn expect_logo_i64_at(name: &str, args: &[Value], index: usize) -> Result<i64> {
    match args.get(index) {
        Some(Value::Int(value)) => Ok(*value),
        _ => Err(MiniError::runtime(format!(
            "function `{}` expects i64 distance/degrees",
            name
        ))),
    }
}

fn expect_logo_string<'a>(name: &str, args: &'a [Value], expected: usize) -> Result<&'a str> {
    expect_logo_arg_count(name, args, expected)?;
    match args.first() {
        Some(Value::String(value)) => Ok(value),
        _ => Err(MiniError::runtime(format!(
            "function `{}` expects String argument",
            name
        ))),
    }
}

fn normalize_logo_heading(value: f64) -> f64 {
    value.rem_euclid(360.0)
}

fn logo_svg(logo: &LogoState, width: i64, height: i64) -> String {
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"{}\" />\n",
        width, height, width, height, logo.background
    );
    for line in &logo.lines {
        svg.push_str(&format!(
            "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" />\n",
            line.x1, line.y1, line.x2, line.y2, line.color, line.width
        ));
    }
    for circle in &logo.circles {
        svg.push_str(&format!(
            "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" />\n",
            circle.x, circle.y, circle.radius, circle.color, circle.width
        ));
    }
    svg.push_str("</svg>\n");
    svg
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
        "logo::forward" | "logo_forward" => "logo_forward",
        "logo::back" | "logo_back" => "logo_back",
        "logo::right" | "logo_right" => "logo_right",
        "logo::left" | "logo_left" => "logo_left",
        "logo::set_position" | "logo_set_position" => "logo_set_position",
        "logo::home" | "logo_home" => "logo_home",
        "logo::heading" | "logo_heading" => "logo_heading",
        "logo::set_heading" | "logo_set_heading" => "logo_set_heading",
        "logo::circle" | "logo_circle" => "logo_circle",
        "logo::width" | "logo_width" => "logo_width",
        "logo::background" | "logo_background" => "logo_background",
        "logo::pen_up" | "logo_pen_up" => "logo_pen_up",
        "logo::pen_down" | "logo_pen_down" => "logo_pen_down",
        "logo::pen_color" | "logo_pen_color" => "logo_pen_color",
        "logo::clear" | "logo_clear" => "logo_clear",
        "logo::save" | "logo_save" => "logo_save",
        "logo::save_with_size" | "logo_save_with_size" => "logo_save_with_size",
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
