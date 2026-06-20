use crate::ast::*;
use crate::error::{MiniError, Result};
use crate::value::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64),
    Bool(bool),
    String(String),
    Unit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    PushConst(usize),
    LoadLocal(usize),
    StoreLocal(usize),
    Pop,
    Binary(ByteBinaryOp),
    Jump(usize),
    JumpIfFalse(usize),
    Call { name: String, argc: usize },
    Return,
}

#[derive(Debug, Clone)]
pub struct BytecodeFunction {
    pub name: String,
    pub params: Vec<String>,
    pub local_count: usize,
    pub constants: Vec<Constant>,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub struct BytecodeProgram {
    pub functions: HashMap<String, BytecodeFunction>,
}

pub fn compile_program(program: &Program) -> Result<BytecodeProgram> {
    let mut compiler = BytecodeCompiler::default();
    compiler.compile(program)
}

#[derive(Default)]
struct BytecodeCompiler;

struct FunctionCompiler<'a> {
    function: &'a Function,
    constants: Vec<Constant>,
    instructions: Vec<Instruction>,
    locals: HashMap<String, usize>,
    local_count: usize,
}

impl BytecodeCompiler {
    fn compile(&mut self, program: &Program) -> Result<BytecodeProgram> {
        if !program.structs.is_empty()
            || !program.enums.is_empty()
            || !program.impls.is_empty()
            || !program.traits.is_empty()
            || !program.modules.is_empty()
            || !program.uses.is_empty()
        {
            return Err(unsupported(
                "bytecode MVP does not support items beyond functions yet",
            ));
        }

        let mut functions = HashMap::new();
        for function in &program.functions {
            let compiled = FunctionCompiler::new(function).compile()?;
            functions.insert(function.name.clone(), compiled);
        }
        Ok(BytecodeProgram { functions })
    }
}

impl<'a> FunctionCompiler<'a> {
    fn new(function: &'a Function) -> Self {
        let mut locals = HashMap::new();
        for (idx, param) in function.params.iter().enumerate() {
            locals.insert(param.name.clone(), idx);
        }
        Self {
            function,
            constants: Vec::new(),
            instructions: Vec::new(),
            locals,
            local_count: function.params.len(),
        }
    }

    fn compile(mut self) -> Result<BytecodeFunction> {
        for statement in &self.function.body.statements {
            self.compile_statement(statement)?;
        }
        if let Some(tail) = &self.function.body.tail {
            self.compile_expr(tail)?;
        } else {
            self.push_const(Constant::Unit)?;
        }
        self.instructions.push(Instruction::Return);
        Ok(BytecodeFunction {
            name: self.function.name.clone(),
            params: self
                .function
                .params
                .iter()
                .map(|p| p.name.clone())
                .collect(),
            local_count: self.local_count,
            constants: self.constants,
            instructions: self.instructions,
        })
    }

    fn compile_statement(&mut self, statement: &Statement) -> Result<()> {
        match statement {
            Statement::Let { pattern, value, .. } => {
                self.compile_expr(value)?;
                let name = match pattern {
                    LetPattern::Ident(name) => name,
                    _ => return Err(unsupported("bytecode MVP supports identifier let only")),
                };
                let slot = self.local_slot(name);
                self.instructions.push(Instruction::StoreLocal(slot));
                Ok(())
            }
            Statement::Assign { target, value, .. } => {
                self.compile_expr(value)?;
                let Expression::Var(name, _) = target else {
                    return Err(unsupported(
                        "bytecode MVP supports variable assignment only",
                    ));
                };
                let Some(slot) = self.locals.get(name).copied() else {
                    return Err(MiniError::runtime(format!("unknown local `{}`", name)));
                };
                self.instructions.push(Instruction::StoreLocal(slot));
                Ok(())
            }
            Statement::Expr(expr) => {
                self.compile_expr(expr)?;
                self.instructions.push(Instruction::Pop);
                Ok(())
            }
            Statement::Return { value, .. } => {
                if let Some(value) = value {
                    self.compile_expr(value)?;
                } else {
                    self.push_const(Constant::Unit)?;
                }
                self.instructions.push(Instruction::Return);
                Ok(())
            }
            Statement::While {
                condition, body, ..
            } => {
                let loop_start = self.instructions.len();
                self.compile_expr(condition)?;
                let jump_out = self.emit_jump_false_placeholder();
                for statement in &body.statements {
                    self.compile_statement(statement)?;
                }
                if let Some(tail) = &body.tail {
                    self.compile_expr(tail)?;
                    self.instructions.push(Instruction::Pop);
                }
                self.instructions.push(Instruction::Jump(loop_start));
                let loop_end = self.instructions.len();
                self.patch_jump(jump_out, loop_end);
                Ok(())
            }
            Statement::Loop { .. }
            | Statement::For { .. }
            | Statement::Break { .. }
            | Statement::Continue { .. } => Err(unsupported(
                "bytecode MVP does not support loop/for/break/continue yet",
            )),
        }
    }

    fn compile_expr(&mut self, expr: &Expression) -> Result<()> {
        match expr {
            Expression::Int(value, _) => self.push_const(Constant::Int(*value)),
            Expression::Bool(value, _) => self.push_const(Constant::Bool(*value)),
            Expression::String(value, _) => self.push_const(Constant::String(value.clone())),
            Expression::Unit(_) => self.push_const(Constant::Unit),
            Expression::Var(name, _) => {
                let Some(slot) = self.locals.get(name).copied() else {
                    return Err(MiniError::runtime(format!("unknown local `{}`", name)));
                };
                self.instructions.push(Instruction::LoadLocal(slot));
                Ok(())
            }
            Expression::Binary {
                op, left, right, ..
            } => {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.instructions.push(Instruction::Binary((*op).into()));
                Ok(())
            }
            Expression::Call { name, args, .. } => {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.instructions.push(Instruction::Call {
                    name: name.clone(),
                    argc: args.len(),
                });
                Ok(())
            }
            Expression::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                self.compile_expr(condition)?;
                let jump_else = self.emit_jump_false_placeholder();
                self.compile_block_as_expr(then_block)?;
                let jump_end = self.emit_jump_placeholder();
                let else_start = self.instructions.len();
                self.patch_jump(jump_else, else_start);
                if let Some(else_block) = else_block {
                    self.compile_block_as_expr(else_block)?;
                } else {
                    self.push_const(Constant::Unit)?;
                }
                let end = self.instructions.len();
                self.patch_jump(jump_end, end);
                Ok(())
            }
            Expression::Block(block) => self.compile_block_as_expr(block),
            Expression::Unary { .. }
            | Expression::Float(_, _)
            | Expression::Tuple(_, _)
            | Expression::Array(_, _)
            | Expression::Vec(_, _)
            | Expression::Range { .. }
            | Expression::MethodCall { .. }
            | Expression::StructLiteral { .. }
            | Expression::EnumLiteral { .. }
            | Expression::Index { .. }
            | Expression::Field { .. }
            | Expression::Match { .. }
            | Expression::Ref { .. }
            | Expression::Deref { .. }
            | Expression::Try { .. } => Err(unsupported("bytecode MVP expression unsupported")),
        }
    }

    fn compile_block_as_expr(&mut self, block: &Block) -> Result<()> {
        for statement in &block.statements {
            self.compile_statement(statement)?;
        }
        if let Some(tail) = &block.tail {
            self.compile_expr(tail)
        } else {
            self.push_const(Constant::Unit)
        }
    }

    fn local_slot(&mut self, name: &str) -> usize {
        if let Some(slot) = self.locals.get(name).copied() {
            return slot;
        }
        let slot = self.local_count;
        self.local_count += 1;
        self.locals.insert(name.to_string(), slot);
        slot
    }

    fn push_const(&mut self, constant: Constant) -> Result<()> {
        let idx = self.constants.len();
        self.constants.push(constant);
        self.instructions.push(Instruction::PushConst(idx));
        Ok(())
    }

    fn emit_jump_false_placeholder(&mut self) -> usize {
        let idx = self.instructions.len();
        self.instructions.push(Instruction::JumpIfFalse(usize::MAX));
        idx
    }

    fn emit_jump_placeholder(&mut self) -> usize {
        let idx = self.instructions.len();
        self.instructions.push(Instruction::Jump(usize::MAX));
        idx
    }

    fn patch_jump(&mut self, instruction: usize, target: usize) {
        match &mut self.instructions[instruction] {
            Instruction::Jump(dest) | Instruction::JumpIfFalse(dest) => *dest = target,
            _ => unreachable!(),
        }
    }
}

impl From<BinaryOp> for ByteBinaryOp {
    fn from(value: BinaryOp) -> Self {
        match value {
            BinaryOp::Add => ByteBinaryOp::Add,
            BinaryOp::Sub => ByteBinaryOp::Sub,
            BinaryOp::Mul => ByteBinaryOp::Mul,
            BinaryOp::Div => ByteBinaryOp::Div,
            BinaryOp::Rem => ByteBinaryOp::Rem,
            BinaryOp::Eq => ByteBinaryOp::Eq,
            BinaryOp::Ne => ByteBinaryOp::Ne,
            BinaryOp::Lt => ByteBinaryOp::Lt,
            BinaryOp::Le => ByteBinaryOp::Le,
            BinaryOp::Gt => ByteBinaryOp::Gt,
            BinaryOp::Ge => ByteBinaryOp::Ge,
            BinaryOp::And => ByteBinaryOp::And,
            BinaryOp::Or => ByteBinaryOp::Or,
        }
    }
}

pub struct BytecodeVm<'a> {
    program: &'a BytecodeProgram,
    output: Vec<String>,
    live_output: bool,
}

impl<'a> BytecodeVm<'a> {
    pub fn new(program: &'a BytecodeProgram) -> Self {
        Self {
            program,
            output: Vec::new(),
            live_output: false,
        }
    }

    pub fn with_live_output(program: &'a BytecodeProgram) -> Self {
        Self {
            program,
            output: Vec::new(),
            live_output: true,
        }
    }

    pub fn run(mut self) -> Result<Vec<String>> {
        self.call_function("main", Vec::new())?;
        Ok(self.output)
    }

    fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value> {
        if name == "print" {
            if args.len() != 1 {
                return Err(MiniError::runtime("function `print` expects 1 argument"));
            }
            if self.live_output {
                println!("{}", args[0]);
            } else {
                self.output.push(args[0].to_string());
            }
            return Ok(Value::Unit);
        }
        let function = self
            .program
            .functions
            .get(name)
            .ok_or_else(|| MiniError::runtime(format!("function `{}` not found", name)))?;
        if function.params.len() != args.len() {
            return Err(MiniError::runtime(format!(
                "function `{}` expects {} arguments",
                name,
                function.params.len()
            )));
        }
        let mut frame = vec![Value::Unit; function.local_count];
        for (idx, arg) in args.into_iter().enumerate() {
            frame[idx] = arg;
        }
        self.run_function(function, frame)
    }

    fn run_function(
        &mut self,
        function: &BytecodeFunction,
        mut locals: Vec<Value>,
    ) -> Result<Value> {
        let mut stack = Vec::<Value>::new();
        let mut ip = 0;
        while ip < function.instructions.len() {
            match &function.instructions[ip] {
                Instruction::PushConst(idx) => {
                    stack.push(constant_to_value(&function.constants[*idx]));
                    ip += 1;
                }
                Instruction::LoadLocal(slot) => {
                    stack.push(locals[*slot].clone());
                    ip += 1;
                }
                Instruction::StoreLocal(slot) => {
                    locals[*slot] = pop_stack(&mut stack)?;
                    ip += 1;
                }
                Instruction::Pop => {
                    pop_stack(&mut stack)?;
                    ip += 1;
                }
                Instruction::Binary(op) => {
                    let right = pop_stack(&mut stack)?;
                    let left = pop_stack(&mut stack)?;
                    stack.push(eval_binary(*op, left, right)?);
                    ip += 1;
                }
                Instruction::Jump(dest) => ip = *dest,
                Instruction::JumpIfFalse(dest) => {
                    let condition = pop_stack(&mut stack)?;
                    if condition == Value::Bool(false) {
                        ip = *dest;
                    } else {
                        ip += 1;
                    }
                }
                Instruction::Call { name, argc } => {
                    let mut args = Vec::new();
                    for _ in 0..*argc {
                        args.push(pop_stack(&mut stack)?);
                    }
                    args.reverse();
                    let value = self.call_function(name, args)?;
                    stack.push(value);
                    ip += 1;
                }
                Instruction::Return => return Ok(stack.pop().unwrap_or(Value::Unit)),
            }
        }
        Ok(Value::Unit)
    }
}

fn constant_to_value(constant: &Constant) -> Value {
    match constant {
        Constant::Int(value) => Value::Int(*value),
        Constant::Bool(value) => Value::Bool(*value),
        Constant::String(value) => Value::String(value.clone()),
        Constant::Unit => Value::Unit,
    }
}

fn pop_stack(stack: &mut Vec<Value>) -> Result<Value> {
    stack
        .pop()
        .ok_or_else(|| MiniError::runtime("bytecode stack underflow"))
}

fn eval_binary(op: ByteBinaryOp, left: Value, right: Value) -> Result<Value> {
    match op {
        ByteBinaryOp::Add => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            _ => Err(MiniError::runtime("bytecode `+` expects matching values")),
        },
        ByteBinaryOp::Sub => int_binary(left, right, |a, b| a - b),
        ByteBinaryOp::Mul => int_binary(left, right, |a, b| a * b),
        ByteBinaryOp::Div => int_binary(left, right, |a, b| a / b),
        ByteBinaryOp::Rem => int_binary(left, right, |a, b| a % b),
        ByteBinaryOp::Eq => Ok(Value::Bool(left == right)),
        ByteBinaryOp::Ne => Ok(Value::Bool(left != right)),
        ByteBinaryOp::Lt => int_compare(left, right, |a, b| a < b),
        ByteBinaryOp::Le => int_compare(left, right, |a, b| a <= b),
        ByteBinaryOp::Gt => int_compare(left, right, |a, b| a > b),
        ByteBinaryOp::Ge => int_compare(left, right, |a, b| a >= b),
        ByteBinaryOp::And => bool_binary(left, right, |a, b| a && b),
        ByteBinaryOp::Or => bool_binary(left, right, |a, b| a || b),
    }
}

fn int_binary(left: Value, right: Value, f: impl FnOnce(i64, i64) -> i64) -> Result<Value> {
    let (Value::Int(left), Value::Int(right)) = (left, right) else {
        return Err(MiniError::runtime("bytecode arithmetic expects i64"));
    };
    Ok(Value::Int(f(left, right)))
}

fn int_compare(left: Value, right: Value, f: impl FnOnce(i64, i64) -> bool) -> Result<Value> {
    let (Value::Int(left), Value::Int(right)) = (left, right) else {
        return Err(MiniError::runtime("bytecode comparison expects i64"));
    };
    Ok(Value::Bool(f(left, right)))
}

fn bool_binary(left: Value, right: Value, f: impl FnOnce(bool, bool) -> bool) -> Result<Value> {
    let (Value::Bool(left), Value::Bool(right)) = (left, right) else {
        return Err(MiniError::runtime("bytecode boolean operator expects bool"));
    };
    Ok(Value::Bool(f(left, right)))
}

fn unsupported(message: &str) -> MiniError {
    MiniError::runtime(message)
}
