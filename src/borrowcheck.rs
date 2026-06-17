use crate::ast::*;
use crate::error::{MiniError, Result, Span};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct VarState {
    ty: Type,
    mutable: bool,
    moved: bool,
    immut_borrows: usize,
    mut_borrowed: bool,
}

#[derive(Debug, Clone)]
enum Loan {
    Imm(String),
    Mut(String),
}

#[derive(Debug, Clone)]
struct Effect {
    ty: Type,
    loan: Option<Loan>,
}

pub struct BorrowChecker<'a> {
    program: &'a Program,
    functions: HashMap<String, FunctionSig>,
}

#[derive(Clone)]
struct FunctionSig {
    params: Vec<Type>,
}

impl<'a> BorrowChecker<'a> {
    pub fn new(program: &'a Program) -> Self {
        Self {
            program,
            functions: HashMap::new(),
        }
    }

    pub fn check(mut self) -> Result<()> {
        for f in &self.program.functions {
            self.functions.insert(
                f.name.clone(),
                FunctionSig {
                    params: f.params.iter().map(|p| p.ty.clone()).collect(),
                },
            );
        }
        for block in &self.program.impls {
            for f in &block.methods {
                self.functions.insert(
                    f.name.clone(),
                    FunctionSig {
                        params: f.params.iter().map(|p| p.ty.clone()).collect(),
                    },
                );
            }
        }
        for f in &self.program.functions {
            self.check_function(f)?;
        }
        for block in &self.program.impls {
            for f in &block.methods {
                self.check_function(f)?;
            }
        }
        Ok(())
    }

    fn check_function(&self, f: &Function) -> Result<()> {
        let mut env = vec![HashMap::new()];
        for p in &f.params {
            env.last_mut().unwrap().insert(
                p.name.clone(),
                VarState {
                    ty: p.ty.clone(),
                    mutable: true,
                    moved: false,
                    immut_borrows: 0,
                    mut_borrowed: false,
                },
            );
        }
        self.check_block(&f.body, &mut env)
    }

    fn check_block(&self, block: &Block, env: &mut Vec<HashMap<String, VarState>>) -> Result<()> {
        env.push(HashMap::new());
        let mut loans = Vec::new();
        for stmt in &block.statements {
            self.check_statement(stmt, env, &mut loans)?;
        }
        if let Some(tail) = &block.tail {
            let effect = self.check_expr(tail, env)?;
            if let Some(loan) = effect.loan {
                self.release_loan(&loan, env);
            }
        }
        for loan in loans.iter().rev() {
            self.release_loan(loan, env);
        }
        env.pop();
        Ok(())
    }

    fn check_statement(
        &self,
        stmt: &Statement,
        env: &mut Vec<HashMap<String, VarState>>,
        scope_loans: &mut Vec<Loan>,
    ) -> Result<()> {
        match stmt {
            Statement::Let {
                name,
                mutable,
                ty,
                value,
                span,
            } => {
                let effect = self.check_expr(value, env)?;
                let final_ty = ty.clone().unwrap_or(effect.ty);
                if let Some(loan) = effect.loan.clone() {
                    scope_loans.push(loan);
                }
                env.last_mut().unwrap().insert(
                    name.clone(),
                    VarState {
                        ty: final_ty,
                        mutable: *mutable,
                        moved: false,
                        immut_borrows: 0,
                        mut_borrowed: false,
                    },
                );
                if env.last().unwrap().get(name).is_none() {
                    return Err(MiniError::borrow(
                        format!("failed to bind `{}`", name),
                        Some(*span),
                    ));
                }
            }
            Statement::Assign {
                target,
                value,
                span,
            } => {
                let assigned_name = match target {
                    Expression::Var(name, _) => Some(name.clone()),
                    _ => None,
                };
                match target {
                    Expression::Var(name, _) => {
                        let state = lookup_mut(env, name).ok_or_else(|| {
                            MiniError::borrow(format!("unknown variable `{}`", name), Some(*span))
                        })?;
                        if !state.mutable {
                            return Err(MiniError::borrow(
                                format!("cannot assign to immutable variable `{}`", name),
                                Some(*span),
                            ));
                        }
                        if state.immut_borrows > 0 || state.mut_borrowed {
                            return Err(MiniError::borrow(
                                format!("cannot assign to `{}` because it is borrowed", name),
                                Some(*span),
                            ));
                        }
                    }
                    Expression::Deref { expr, .. } => {
                        self.check_mut_deref_target(expr, env, *span)?;
                    }
                    Expression::Index { target, index, .. } => {
                        self.check_assignable_base(target, env, *span)?;
                        let effect = self.check_expr(index, env)?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    }
                    Expression::Field { target, .. } => {
                        self.check_assignable_base(target, env, *span)?;
                    }
                    _ => return Err(MiniError::borrow(
                        "assignment target must be variable, mutable dereference, index, or field",
                        Some(*span),
                    )),
                }
                let effect = self.check_expr(value, env)?;
                if let Some(loan) = effect.loan {
                    self.release_loan(&loan, env);
                }
                if let Some(name) = assigned_name {
                    if let Some(state) = lookup_mut(env, &name) {
                        state.moved = false;
                    }
                }
            }
            Statement::Expr(expr) => {
                let effect = self.check_expr(expr, env)?;
                if let Some(loan) = effect.loan {
                    self.release_loan(&loan, env);
                }
            }
            Statement::Return { value, .. } => {
                if let Some(value) = value {
                    let effect = self.check_expr(value, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                }
            }
            Statement::Break { .. } | Statement::Continue { .. } => {}
            Statement::While {
                condition, body, ..
            } => {
                let effect = self.check_expr(condition, env)?;
                if let Some(loan) = effect.loan {
                    self.release_loan(&loan, env);
                }
                self.check_block(body, env)?;
            }
            Statement::Loop { body, .. } => {
                self.check_block(body, env)?;
            }
            Statement::For {
                name,
                iterable,
                body,
                ..
            } => {
                let effect = self.check_expr(iterable, env)?;
                let item_ty = match effect.ty {
                    Type::Array(item, _) | Type::Vec(item) => *item,
                    Type::String => Type::String,
                    Type::Range => Type::I64,
                    other => other,
                };
                if let Some(loan) = effect.loan {
                    self.release_loan(&loan, env);
                }
                env.push(HashMap::new());
                env.last_mut().unwrap().insert(
                    name.clone(),
                    VarState {
                        ty: item_ty,
                        mutable: false,
                        moved: false,
                        immut_borrows: 0,
                        mut_borrowed: false,
                    },
                );
                self.check_block(body, env)?;
                env.pop();
            }
        }
        Ok(())
    }

    fn check_expr(
        &self,
        expr: &Expression,
        env: &mut Vec<HashMap<String, VarState>>,
    ) -> Result<Effect> {
        match expr {
            Expression::Int(_, _) => Ok(Effect {
                ty: Type::I64,
                loan: None,
            }),
            Expression::Float(_, _) => Ok(Effect {
                ty: Type::F64,
                loan: None,
            }),
            Expression::Bool(_, _) => Ok(Effect {
                ty: Type::Bool,
                loan: None,
            }),
            Expression::String(_, _) => Ok(Effect {
                ty: Type::String,
                loan: None,
            }),
            Expression::Range { start, end, .. } => {
                let start_effect = self.check_expr(start, env)?;
                if let Some(loan) = start_effect.loan {
                    self.release_loan(&loan, env);
                }
                let end_effect = self.check_expr(end, env)?;
                if let Some(loan) = end_effect.loan {
                    self.release_loan(&loan, env);
                }
                Ok(Effect {
                    ty: Type::Range,
                    loan: None,
                })
            }
            Expression::Tuple(items, _) => {
                let mut types = Vec::new();
                for item in items {
                    let effect = self.check_expr(item, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                    types.push(effect.ty);
                }
                Ok(Effect {
                    ty: Type::Tuple(types),
                    loan: None,
                })
            }
            Expression::Array(items, _) => {
                let mut elem_ty = None;
                for item in items {
                    let effect = self.check_expr(item, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                    elem_ty.get_or_insert(effect.ty);
                }
                Ok(Effect {
                    ty: Type::Array(Box::new(elem_ty.unwrap_or(Type::Unit)), items.len()),
                    loan: None,
                })
            }
            Expression::Vec(items, _) => {
                let mut elem_ty = None;
                for item in items {
                    let effect = self.check_expr(item, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                    elem_ty.get_or_insert(effect.ty);
                }
                Ok(Effect {
                    ty: Type::Vec(Box::new(elem_ty.unwrap_or(Type::Unit))),
                    loan: None,
                })
            }
            Expression::Unit(_) => Ok(Effect {
                ty: Type::Unit,
                loan: None,
            }),
            Expression::Var(name, span) => {
                let state = lookup_mut(env, name).ok_or_else(|| {
                    MiniError::borrow(format!("unknown variable `{}`", name), Some(*span))
                })?;
                if state.moved {
                    return Err(MiniError::borrow(
                        format!("use of moved value `{}`", name),
                        Some(*span),
                    ));
                }
                let ty = state.ty.clone();
                if !ty.is_copy() {
                    if state.immut_borrows > 0 || state.mut_borrowed {
                        return Err(MiniError::borrow(
                            format!("cannot move `{}` because it is borrowed", name),
                            Some(*span),
                        ));
                    }
                    state.moved = true;
                }
                Ok(Effect { ty, loan: None })
            }
            Expression::Unary { op, expr, .. } => {
                let effect = self.check_expr(expr, env)?;
                if let Some(loan) = effect.loan {
                    self.release_loan(&loan, env);
                }
                let ty = match op {
                    UnaryOp::Neg => effect.ty,
                    UnaryOp::Not => Type::Bool,
                };
                Ok(Effect { ty, loan: None })
            }
            Expression::Deref { expr, .. } => {
                let effect = self.check_expr(expr, env)?;
                let ty = match effect.ty {
                    Type::Ref(inner) | Type::MutRef(inner) => *inner,
                    other => other,
                };
                if let Some(loan) = effect.loan {
                    self.release_loan(&loan, env);
                }
                Ok(Effect { ty, loan: None })
            }
            Expression::Binary {
                op, left, right, ..
            } => {
                let left = self.check_expr(left, env)?;
                if let Some(loan) = left.loan {
                    self.release_loan(&loan, env);
                }
                let right = self.check_expr(right, env)?;
                if let Some(loan) = right.loan {
                    self.release_loan(&loan, env);
                }
                let ty = match op {
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Rem => left.ty,
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::And
                    | BinaryOp::Or => Type::Bool,
                };
                Ok(Effect { ty, loan: None })
            }
            Expression::Call { name, args, span } => {
                let name = builtin_alias(name);
                if name == "len" {
                    if let Some(Expression::Var(var, arg_span)) = args.first() {
                        let state = lookup(env, var).ok_or_else(|| {
                            MiniError::borrow(
                                format!("unknown variable `{}`", var),
                                Some(*arg_span),
                            )
                        })?;
                        if state.moved {
                            return Err(MiniError::borrow(
                                format!("use of moved value `{}`", var),
                                Some(*arg_span),
                            ));
                        }
                        return Ok(Effect {
                            ty: Type::I64,
                            loan: None,
                        });
                    }
                }
                if name == "print" {
                    if let Some(Expression::Var(var, arg_span)) = args.first() {
                        let state = lookup(env, var).ok_or_else(|| {
                            MiniError::borrow(
                                format!("unknown variable `{}`", var),
                                Some(*arg_span),
                            )
                        })?;
                        if state.moved {
                            return Err(MiniError::borrow(
                                format!("use of moved value `{}`", var),
                                Some(*arg_span),
                            ));
                        }
                        return Ok(Effect {
                            ty: Type::Unit,
                            loan: None,
                        });
                    }
                }
                if name == "args" || name == "clock_ms" {
                    return Ok(Effect {
                        ty: if name == "args" {
                            Type::Vec(Box::new(Type::String))
                        } else {
                            Type::I64
                        },
                        loan: None,
                    });
                }
                if name == "clear" {
                    return Ok(Effect {
                        ty: Type::Unit,
                        loan: None,
                    });
                }
                if name == "read_key" {
                    return Ok(Effect {
                        ty: Type::String,
                        loan: None,
                    });
                }
                if name == "sleep_ms" || name == "rand_i64" || name == "color" {
                    for arg in args {
                        let effect = self.check_expr(arg, env)?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    }
                    return Ok(Effect {
                        ty: match name {
                            "rand_i64" => Type::I64,
                            "color" => Type::String,
                            _ => Type::Unit,
                        },
                        loan: None,
                    });
                }
                if name == "input"
                    || name == "read_i64_alias"
                    || name == "read_f64_alias"
                    || name == "parse_f64"
                    || name == "parse_i64"
                    || name == "unwrap_or"
                {
                    for arg in args {
                        let effect = self.check_expr(arg, env)?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    }
                    return Ok(Effect {
                        ty: match name {
                            "input" => Type::String,
                            "read_i64_alias" => Type::I64,
                            "read_f64_alias" => Type::F64,
                            "parse_f64" => {
                                Type::Result(Box::new(Type::F64), Box::new(Type::String))
                            }
                            "parse_i64" => {
                                Type::Result(Box::new(Type::I64), Box::new(Type::String))
                            }
                            _ => Type::Unit,
                        },
                        loan: None,
                    });
                }
                if name == "contains" {
                    for arg in args {
                        if let Expression::Var(var, arg_span) = arg {
                            let state = lookup(env, var).ok_or_else(|| {
                                MiniError::borrow(
                                    format!("unknown variable `{}`", var),
                                    Some(*arg_span),
                                )
                            })?;
                            if state.moved {
                                return Err(MiniError::borrow(
                                    format!("use of moved value `{}`", var),
                                    Some(*arg_span),
                                ));
                            }
                        } else {
                            let effect = self.check_expr(arg, env)?;
                            if let Some(loan) = effect.loan {
                                self.release_loan(&loan, env);
                            }
                        }
                    }
                    return Ok(Effect {
                        ty: Type::Bool,
                        loan: None,
                    });
                }
                if name == "__format_macro" || name == "__print_macro" || name == "__println_macro"
                {
                    for arg in args {
                        if let Expression::Var(var, arg_span) = arg {
                            let state = lookup(env, var).ok_or_else(|| {
                                MiniError::borrow(
                                    format!("unknown variable `{}`", var),
                                    Some(*arg_span),
                                )
                            })?;
                            if state.moved {
                                return Err(MiniError::borrow(
                                    format!("use of moved value `{}`", var),
                                    Some(*arg_span),
                                ));
                            }
                        } else {
                            let effect = self.check_expr(arg, env)?;
                            if let Some(loan) = effect.loan {
                                self.release_loan(&loan, env);
                            }
                        }
                    }
                    return Ok(Effect {
                        ty: if name == "__format_macro" {
                            Type::String
                        } else {
                            Type::Unit
                        },
                        loan: None,
                    });
                }
                if name == "env" || name == "concat" || name == "read_file" || name == "write_file"
                {
                    for arg in args {
                        let effect = self.check_expr(arg, env)?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    }
                    return Ok(Effect {
                        ty: if name == "read_file" {
                            Type::Result(Box::new(Type::String), Box::new(Type::String))
                        } else if name == "env" || name == "concat" {
                            Type::String
                        } else {
                            Type::Unit
                        },
                        loan: None,
                    });
                }
                let params = self
                    .functions
                    .get(name)
                    .map(|s| s.params.clone())
                    .unwrap_or_default();
                let mut active = Vec::new();
                for (idx, arg) in args.iter().enumerate() {
                    let effect = self.check_expr(arg, env)?;
                    if let Some(loan) = effect.loan {
                        active.push(loan);
                    }
                    if matches!(params.get(idx), Some(Type::MutRef(_)))
                        && !matches!(effect.ty, Type::MutRef(_))
                    {
                        return Err(MiniError::borrow(
                            "expected mutable reference argument",
                            Some(*span),
                        ));
                    }
                }
                for loan in active.iter().rev() {
                    self.release_loan(loan, env);
                }
                Ok(Effect {
                    ty: if name == "len" { Type::I64 } else { Type::Unit },
                    loan: None,
                })
            }
            Expression::MethodCall {
                receiver,
                name,
                args,
                span,
            } => {
                let recv_ty = self.peek_expr_type(receiver, env)?;
                if matches!(
                    name.as_str(),
                    "len" | "trim" | "is_some" | "is_none" | "is_ok" | "is_err"
                ) || (name == "unwrap_or"
                    && matches!(recv_ty, Type::Option(_) | Type::Result(_, _)))
                {
                    if let Expression::Var(var, var_span) = &**receiver {
                        let state = lookup(env, var).ok_or_else(|| {
                            MiniError::borrow(
                                format!("unknown variable `{}`", var),
                                Some(*var_span),
                            )
                        })?;
                        if state.moved {
                            return Err(MiniError::borrow(
                                format!("use of moved value `{}`", var),
                                Some(*var_span),
                            ));
                        }
                    } else {
                        let effect = self.check_expr(receiver, env)?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    }
                    for arg in args {
                        let effect = self.check_expr(arg, env)?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    }
                    return Ok(Effect {
                        ty: match name.as_str() {
                            "len" => Type::I64,
                            "trim" => Type::String,
                            "is_some" | "is_none" | "is_ok" | "is_err" => Type::Bool,
                            "unwrap_or" => match recv_ty {
                                Type::Option(inner) | Type::Result(inner, _) => *inner,
                                _ => Type::Unit,
                            },
                            _ => Type::Unit,
                        },
                        loan: None,
                    });
                }
                if name == "push" || name == "push_str" || name == "pop" {
                    let Expression::Var(var, var_span) = &**receiver else {
                        return Err(MiniError::borrow(
                            format!("method `{}` expects a variable receiver", name),
                            Some(*span),
                        ));
                    };
                    let state = lookup(env, var).ok_or_else(|| {
                        MiniError::borrow(format!("unknown variable `{}`", var), Some(*var_span))
                    })?;
                    if state.moved {
                        return Err(MiniError::borrow(
                            format!("use of moved value `{}`", var),
                            Some(*var_span),
                        ));
                    }
                    if !state.mutable {
                        return Err(MiniError::borrow(
                            format!(
                                "cannot call mutating method `{}` on immutable `{}`",
                                name, var
                            ),
                            Some(*var_span),
                        ));
                    }
                    if state.immut_borrows > 0 || state.mut_borrowed {
                        return Err(MiniError::borrow(
                            format!("cannot mutate `{}` because it is borrowed", var),
                            Some(*var_span),
                        ));
                    }
                    for arg in args {
                        let effect = self.check_expr(arg, env)?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    }
                    let ty = if name == "pop" {
                        match recv_ty {
                            Type::Vec(item) => Type::Option(item),
                            _ => Type::Unit,
                        }
                    } else {
                        Type::Unit
                    };
                    return Ok(Effect { ty, loan: None });
                }
                let type_name = match &recv_ty {
                    Type::Struct(name) | Type::Enum(name) => name.clone(),
                    Type::Ref(inner) | Type::MutRef(inner) => match &**inner {
                        Type::Struct(name) | Type::Enum(name) => name.clone(),
                        _ => String::new(),
                    },
                    _ => String::new(),
                };
                let method_name = format!("{}::{}", type_name, name);
                let params = self
                    .functions
                    .get(&method_name)
                    .map(|sig| sig.params.clone())
                    .unwrap_or_default();
                if matches!(params.first(), Some(Type::Ref(_)) | Some(Type::MutRef(_))) {
                    if let Expression::Var(var, _) = &**receiver {
                        let mutable = matches!(params.first(), Some(Type::MutRef(_)));
                        let effect = self.check_expr(
                            &Expression::Ref {
                                mutable,
                                expr: Box::new(Expression::Var(var.clone(), receiver.span())),
                                span: receiver.span(),
                            },
                            env,
                        )?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    } else {
                        let effect = self.check_expr(receiver, env)?;
                        if let Some(loan) = effect.loan {
                            self.release_loan(&loan, env);
                        }
                    }
                } else {
                    let effect = self.check_expr(receiver, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                }
                for arg in args {
                    let effect = self.check_expr(arg, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                }
                Ok(Effect {
                    ty: Type::Unit,
                    loan: None,
                })
            }
            Expression::StructLiteral { name, fields, .. } => {
                for (_, value) in fields {
                    let effect = self.check_expr(value, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                }
                Ok(Effect {
                    ty: Type::Struct(name.clone()),
                    loan: None,
                })
            }
            Expression::EnumLiteral {
                enum_name, value, ..
            } => {
                if let Some(value) = value {
                    let effect = self.check_expr(value, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                }
                Ok(Effect {
                    ty: Type::Enum(enum_name.clone()),
                    loan: None,
                })
            }
            Expression::Index { target, index, .. } => {
                if let Expression::Var(name, span) = &**target {
                    let state = lookup(env, name).ok_or_else(|| {
                        MiniError::borrow(format!("unknown variable `{}`", name), Some(*span))
                    })?;
                    if state.moved {
                        return Err(MiniError::borrow(
                            format!("use of moved value `{}`", name),
                            Some(*span),
                        ));
                    }
                    let ty = match &state.ty {
                        Type::Array(inner, _) | Type::Vec(inner) => *inner.clone(),
                        other => other.clone(),
                    };
                    let index = self.check_expr(index, env)?;
                    if let Some(loan) = index.loan {
                        self.release_loan(&loan, env);
                    }
                    return Ok(Effect { ty, loan: None });
                }
                let target = self.check_expr(target, env)?;
                if let Some(loan) = target.loan {
                    self.release_loan(&loan, env);
                }
                let index = self.check_expr(index, env)?;
                if let Some(loan) = index.loan {
                    self.release_loan(&loan, env);
                }
                let ty = match target.ty {
                    Type::Array(inner, _) | Type::Vec(inner) => *inner,
                    other => other,
                };
                Ok(Effect { ty, loan: None })
            }
            Expression::Field { target, field, .. } => {
                if let Expression::Var(name, span) = &**target {
                    let state = lookup(env, name).ok_or_else(|| {
                        MiniError::borrow(format!("unknown variable `{}`", name), Some(*span))
                    })?;
                    if state.moved {
                        return Err(MiniError::borrow(
                            format!("use of moved value `{}`", name),
                            Some(*span),
                        ));
                    }
                    let base_ty = match &state.ty {
                        Type::Ref(inner) | Type::MutRef(inner) => &**inner,
                        other => other,
                    };
                    let ty = match base_ty {
                        Type::Tuple(items) => field
                            .parse::<usize>()
                            .ok()
                            .and_then(|idx| items.get(idx).cloned())
                            .unwrap_or(Type::Unit),
                        Type::Struct(_) => Type::Unit,
                        other => other.clone(),
                    };
                    return Ok(Effect { ty, loan: None });
                }
                let target = self.check_expr(target, env)?;
                if let Some(loan) = target.loan {
                    self.release_loan(&loan, env);
                }
                let ty = match target.ty {
                    Type::Tuple(items) => field
                        .parse::<usize>()
                        .ok()
                        .and_then(|idx| items.get(idx).cloned())
                        .unwrap_or(Type::Unit),
                    other => other,
                };
                Ok(Effect { ty, loan: None })
            }
            Expression::Block(block) => {
                self.check_block(block, env)?;
                Ok(Effect {
                    ty: Type::Unit,
                    loan: None,
                })
            }
            Expression::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let cond = self.check_expr(condition, env)?;
                if let Some(loan) = cond.loan {
                    self.release_loan(&loan, env);
                }
                self.check_block(then_block, env)?;
                if let Some(block) = else_block {
                    self.check_block(block, env)?;
                }
                Ok(Effect {
                    ty: Type::Unit,
                    loan: None,
                })
            }
            Expression::Match { value, arms, .. } => {
                let effect = self.check_expr(value, env)?;
                if let Some(loan) = effect.loan {
                    self.release_loan(&loan, env);
                }
                for arm in arms {
                    env.push(HashMap::new());
                    if let Pattern::EnumVariant {
                        binding: Some(binding),
                        ..
                    } = &arm.pattern
                    {
                        env.last_mut().unwrap().insert(
                            binding.clone(),
                            VarState {
                                ty: Type::Unit,
                                mutable: false,
                                moved: false,
                                immut_borrows: 0,
                                mut_borrowed: false,
                            },
                        );
                    }
                    let effect = self.check_expr(&arm.body, env)?;
                    if let Some(loan) = effect.loan {
                        self.release_loan(&loan, env);
                    }
                    env.pop();
                }
                Ok(Effect {
                    ty: Type::Unit,
                    loan: None,
                })
            }
            Expression::Ref {
                mutable,
                expr,
                span,
            } => {
                let Expression::Var(name, _) = &**expr else {
                    return Err(MiniError::borrow(
                        "reference target must be variable",
                        Some(*span),
                    ));
                };
                let state = lookup_mut(env, name).ok_or_else(|| {
                    MiniError::borrow(format!("unknown variable `{}`", name), Some(*span))
                })?;
                if state.moved {
                    return Err(MiniError::borrow(
                        format!("borrow of moved value `{}`", name),
                        Some(*span),
                    ));
                }
                if *mutable {
                    if !state.mutable {
                        return Err(MiniError::borrow(
                            format!("cannot mutably borrow immutable variable `{}`", name),
                            Some(*span),
                        ));
                    }
                    if state.immut_borrows > 0 {
                        return Err(MiniError::borrow(
                            format!(
                                "cannot mutably borrow `{}` while it is immutably borrowed",
                                name
                            ),
                            Some(*span),
                        ));
                    }
                    if state.mut_borrowed {
                        return Err(MiniError::borrow(
                            format!("cannot mutably borrow `{}` more than once", name),
                            Some(*span),
                        ));
                    }
                    state.mut_borrowed = true;
                    Ok(Effect {
                        ty: Type::MutRef(Box::new(state.ty.clone())),
                        loan: Some(Loan::Mut(name.clone())),
                    })
                } else {
                    if state.mut_borrowed {
                        return Err(MiniError::borrow(
                            format!(
                                "cannot immutably borrow `{}` while it is mutably borrowed",
                                name
                            ),
                            Some(*span),
                        ));
                    }
                    state.immut_borrows += 1;
                    Ok(Effect {
                        ty: Type::Ref(Box::new(state.ty.clone())),
                        loan: Some(Loan::Imm(name.clone())),
                    })
                }
            }
        }
    }

    fn check_mut_deref_target(
        &self,
        expr: &Expression,
        env: &mut Vec<HashMap<String, VarState>>,
        span: Span,
    ) -> Result<()> {
        match expr {
            Expression::Var(name, _) => {
                let state = lookup(env, name).ok_or_else(|| {
                    MiniError::borrow(format!("unknown variable `{}`", name), Some(span))
                })?;
                if matches!(state.ty, Type::MutRef(_)) && !state.moved {
                    Ok(())
                } else {
                    Err(MiniError::borrow(
                        "dereference assignment requires mutable reference",
                        Some(span),
                    ))
                }
            }
            _ => Err(MiniError::borrow(
                "dereference assignment requires mutable reference variable",
                Some(span),
            )),
        }
    }

    fn peek_expr_type(
        &self,
        expr: &Expression,
        env: &mut Vec<HashMap<String, VarState>>,
    ) -> Result<Type> {
        match expr {
            Expression::Var(name, span) => lookup(env, name)
                .map(|state| state.ty.clone())
                .ok_or_else(|| {
                    MiniError::borrow(format!("unknown variable `{}`", name), Some(*span))
                }),
            _ => Ok(Type::Unit),
        }
    }

    fn check_assignable_base(
        &self,
        target: &Expression,
        env: &mut Vec<HashMap<String, VarState>>,
        span: Span,
    ) -> Result<()> {
        match target {
            Expression::Var(name, _) => {
                let state = lookup_mut(env, name).ok_or_else(|| {
                    MiniError::borrow(format!("unknown variable `{}`", name), Some(span))
                })?;
                if !state.mutable {
                    return Err(MiniError::borrow(
                        format!("cannot assign through immutable variable `{}`", name),
                        Some(span),
                    ));
                }
                if state.immut_borrows > 0 || state.mut_borrowed {
                    return Err(MiniError::borrow(
                        format!("cannot assign to `{}` because it is borrowed", name),
                        Some(span),
                    ));
                }
                Ok(())
            }
            Expression::Field { target, .. } | Expression::Index { target, .. } => {
                self.check_assignable_base(target, env, span)
            }
            Expression::Deref { expr, .. } => self.check_mut_deref_target(expr, env, span),
            _ => Err(MiniError::borrow("invalid assignment target", Some(span))),
        }
    }

    fn release_loan(&self, loan: &Loan, env: &mut Vec<HashMap<String, VarState>>) {
        match loan {
            Loan::Imm(name) => {
                if let Some(state) = lookup_mut(env, name) {
                    state.immut_borrows = state.immut_borrows.saturating_sub(1);
                }
            }
            Loan::Mut(name) => {
                if let Some(state) = lookup_mut(env, name) {
                    state.mut_borrowed = false;
                }
            }
        }
    }
}

fn lookup<'a>(env: &'a [HashMap<String, VarState>], name: &str) -> Option<&'a VarState> {
    env.iter().rev().find_map(|scope| scope.get(name))
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

fn lookup_mut<'a>(
    env: &'a mut [HashMap<String, VarState>],
    name: &str,
) -> Option<&'a mut VarState> {
    env.iter_mut().rev().find_map(|scope| scope.get_mut(name))
}

#[cfg(test)]
mod tests {
    use crate::{check_program, parse_source};

    #[test]
    fn catches_move_error() {
        let p = parse_source(
            r#"fn main(){ let name:String = "Ruby"; let other = name; print(name); }"#,
        )
        .unwrap();
        let err = check_program(&p).unwrap_err().to_string();
        assert!(err.contains("use of moved value `name`"));
    }

    #[test]
    fn catches_borrow_conflict() {
        let p = parse_source("fn main(){ let mut x:i64=1; let a=&x; let b=&mut x; }").unwrap();
        let err = check_program(&p).unwrap_err().to_string();
        assert!(err.contains("cannot mutably borrow `x` while it is immutably borrowed"));
    }
}
