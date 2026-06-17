use crate::ast::*;
use crate::error::{MiniError, Result, Span};
use std::collections::HashMap;

#[derive(Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
}

#[derive(Clone)]
struct FnSig {
    params: Vec<Type>,
    ret: Type,
}

pub struct TypeChecker<'a> {
    program: &'a Program,
    functions: HashMap<String, FnSig>,
    structs: HashMap<String, Vec<StructField>>,
    enums: HashMap<String, Vec<EnumVariant>>,
    traits: HashMap<String, Vec<TraitMethod>>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(program: &'a Program) -> Self {
        Self {
            program,
            functions: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            traits: HashMap::new(),
        }
    }

    pub fn check(mut self) -> Result<()> {
        for def in &self.program.traits {
            self.traits.insert(def.name.clone(), def.methods.clone());
        }
        for def in &self.program.structs {
            self.structs.insert(def.name.clone(), def.fields.clone());
            for field in &def.fields {
                if self.resolve_type(&field.ty).contains_ref() {
                    return Err(MiniError::type_error(
                        format!(
                            "struct field `{}` cannot store a reference in Rust Mini",
                            field.name
                        ),
                        Some(def.span),
                    ));
                }
            }
        }
        for def in &self.program.enums {
            self.enums.insert(def.name.clone(), def.variants.clone());
            for variant in &def.variants {
                if variant
                    .payload
                    .as_ref()
                    .is_some_and(|ty| self.resolve_type(ty).contains_ref())
                {
                    return Err(MiniError::type_error(
                        format!(
                            "enum variant `{}` cannot store a reference in Rust Mini",
                            variant.name
                        ),
                        Some(def.span),
                    ));
                }
            }
        }
        self.functions.insert(
            "print".to_string(),
            FnSig {
                params: vec![Type::String],
                ret: Type::Unit,
            },
        );
        for function in &self.program.functions {
            self.functions.insert(
                function.name.clone(),
                FnSig {
                    params: function
                        .params
                        .iter()
                        .map(|p| self.resolve_type(&p.ty))
                        .collect(),
                    ret: self.resolve_type(&function.ret_type),
                },
            );
        }
        for block in &self.program.impls {
            for method in &block.methods {
                self.functions.insert(
                    method.name.clone(),
                    FnSig {
                        params: method
                            .params
                            .iter()
                            .map(|p| self.resolve_type(&p.ty))
                            .collect(),
                        ret: self.resolve_type(&method.ret_type),
                    },
                );
            }
        }
        for function in &self.program.functions {
            self.check_function(function)?;
        }
        for block in &self.program.impls {
            for method in &block.methods {
                self.check_function(method)?;
            }
            self.check_trait_impl(block)?;
        }
        Ok(())
    }

    fn check_trait_impl(&self, block: &ImplBlock) -> Result<()> {
        let Some(trait_name) = &block.trait_name else {
            return Ok(());
        };
        let required = self.traits.get(trait_name).ok_or_else(|| {
            MiniError::type_error(format!("unknown trait `{}`", trait_name), Some(block.span))
        })?;
        for trait_method in required {
            let full_name = format!("{}::{}", block.target, trait_method.name);
            let Some(method) = block.methods.iter().find(|m| m.name == full_name) else {
                return Err(MiniError::type_error(
                    format!(
                        "impl `{}` for `{}` is missing method `{}`",
                        trait_name, block.target, trait_method.name
                    ),
                    Some(block.span),
                ));
            };
            if method.params.len() != trait_method.params.len() {
                return Err(MiniError::type_error(
                    format!("method `{}` has wrong parameter count", trait_method.name),
                    Some(method.span),
                ));
            }
            for (actual, expected) in method.params.iter().zip(&trait_method.params) {
                let expected_ty = replace_self_type(&expected.ty, &block.target);
                let actual_ty = self.resolve_type(&actual.ty);
                self.expect_type(&expected_ty, &actual_ty, Some(method.span))?;
            }
            let expected_ret = replace_self_type(&trait_method.ret_type, &block.target);
            let actual_ret = self.resolve_type(&method.ret_type);
            self.expect_type(&expected_ret, &actual_ret, Some(method.span))?;
        }
        Ok(())
    }

    fn check_function(&self, function: &Function) -> Result<()> {
        let ret_ty = self.resolve_type(&function.ret_type);
        if ret_ty.contains_ref() {
            return Err(MiniError::type_error(
                format!(
                    "function `{}` cannot return a reference in Rust Mini",
                    function.name
                ),
                Some(function.span),
            ));
        }
        let mut env = Vec::<HashMap<String, VarInfo>>::new();
        env.push(HashMap::new());
        for param in &function.params {
            env.last_mut().unwrap().insert(
                param.name.clone(),
                VarInfo {
                    ty: self.resolve_type(&param.ty),
                    mutable: true,
                },
            );
        }
        let ty = self.check_block(&function.body, &mut env, &ret_ty, 0)?;
        if ret_ty != Type::Unit && ty != ret_ty && !block_guaranteed_returns(&function.body) {
            return Err(MiniError::type_error(
                format!(
                    "expected function `{}` to return `{:?}`, found `{:?}`",
                    function.name, ret_ty, ty
                ),
                Some(function.span),
            ));
        }
        Ok(())
    }

    fn check_block(
        &self,
        block: &Block,
        env: &mut Vec<HashMap<String, VarInfo>>,
        ret_ty: &Type,
        loop_depth: usize,
    ) -> Result<Type> {
        env.push(HashMap::new());
        for statement in &block.statements {
            self.check_statement(statement, env, ret_ty, loop_depth)?;
        }
        let ty = if let Some(tail) = &block.tail {
            self.check_expr(tail, env, loop_depth)?
        } else {
            Type::Unit
        };
        env.pop();
        Ok(ty)
    }

    fn check_statement(
        &self,
        statement: &Statement,
        env: &mut Vec<HashMap<String, VarInfo>>,
        ret_ty: &Type,
        loop_depth: usize,
    ) -> Result<()> {
        match statement {
            Statement::Let {
                name,
                mutable,
                ty,
                value,
                span,
            } => {
                let value_ty = self.check_expr(value, env, loop_depth)?;
                if let Some(expected) = ty {
                    let expected = self.resolve_type(expected);
                    self.expect_type(&expected, &value_ty, Some(*span))?;
                }
                env.last_mut().unwrap().insert(
                    name.clone(),
                    VarInfo {
                        ty: ty
                            .as_ref()
                            .map(|ty| self.resolve_type(ty))
                            .unwrap_or(value_ty),
                        mutable: *mutable,
                    },
                );
            }
            Statement::Assign {
                target,
                value,
                span,
            } => {
                let value_ty = self.check_expr(value, env, loop_depth)?;
                match target {
                    Expression::Var(name, _) => {
                        let info = lookup(env, name).ok_or_else(|| {
                            MiniError::type_error(
                                format!("unknown variable `{}`", name),
                                Some(*span),
                            )
                        })?;
                        if !info.mutable {
                            return Err(MiniError::type_error(
                                format!("cannot assign to immutable variable `{}`", name),
                                Some(*span),
                            ));
                        }
                        self.expect_type(&info.ty, &value_ty, Some(*span))?;
                    }
                    Expression::Deref { expr, .. } => match self
                        .check_expr(expr, env, loop_depth)?
                    {
                        Type::MutRef(inner) => self.expect_type(&inner, &value_ty, Some(*span))?,
                        other => {
                            return Err(MiniError::type_error(
                                format!("cannot assign through dereference of `{:?}`", other),
                                Some(*span),
                            ));
                        }
                    },
                    Expression::Index { target, index, .. } => {
                        let target_ty = self.check_expr(target, env, loop_depth)?;
                        let index_ty = self.check_expr(index, env, loop_depth)?;
                        self.expect_type(&Type::I64, &index_ty, Some(index.span()))?;
                        match target_ty {
                            Type::Array(inner, _) => {
                                self.expect_type(&inner, &value_ty, Some(*span))?
                            }
                            Type::Vec(inner) => self.expect_type(&inner, &value_ty, Some(*span))?,
                            other => {
                                return Err(MiniError::type_error(
                                    format!("cannot index-assign `{:?}`", other),
                                    Some(*span),
                                ));
                            }
                        }
                    }
                    Expression::Field { target, field, .. } => {
                        let target_ty = self.check_expr(target, env, loop_depth)?;
                        let target_ty = match target_ty {
                            Type::Ref(inner) | Type::MutRef(inner) => *inner,
                            other => other,
                        };
                        match target_ty {
                            Type::Tuple(items) => {
                                let index = field.parse::<usize>().map_err(|_| {
                                    MiniError::type_error(
                                        "tuple field must be numeric",
                                        Some(*span),
                                    )
                                })?;
                                let Some(field_ty) = items.get(index) else {
                                    return Err(MiniError::type_error(
                                        format!("tuple field `{}` out of range", field),
                                        Some(*span),
                                    ));
                                };
                                self.expect_type(field_ty, &value_ty, Some(*span))?;
                            }
                            Type::Struct(name) => {
                                let fields = self.structs.get(&name).ok_or_else(|| {
                                    MiniError::type_error(
                                        format!("unknown struct `{}`", name),
                                        Some(*span),
                                    )
                                })?;
                                let Some(def) = fields.iter().find(|def| def.name == *field) else {
                                    return Err(MiniError::type_error(
                                        format!("unknown field `{}` for struct `{}`", field, name),
                                        Some(*span),
                                    ));
                                };
                                let field_ty = self.resolve_type(&def.ty);
                                self.expect_type(&field_ty, &value_ty, Some(*span))?;
                            }
                            other => {
                                return Err(MiniError::type_error(
                                    format!("cannot field-assign `{:?}`", other),
                                    Some(*span),
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err(MiniError::type_error(
                            "assignment target must be variable, dereference, index, or field",
                            Some(*span),
                        ))
                    }
                }
            }
            Statement::Expr(expr) => {
                self.check_expr(expr, env, loop_depth)?;
            }
            Statement::Return { value, span } => {
                let value_ty = if let Some(value) = value {
                    self.check_expr(value, env, loop_depth)?
                } else {
                    Type::Unit
                };
                self.expect_type(ret_ty, &value_ty, Some(*span))?;
            }
            Statement::Break { span } => {
                if loop_depth == 0 {
                    return Err(MiniError::type_error("`break` outside loop", Some(*span)));
                }
            }
            Statement::Continue { span } => {
                if loop_depth == 0 {
                    return Err(MiniError::type_error(
                        "`continue` outside loop",
                        Some(*span),
                    ));
                }
            }
            Statement::While {
                condition,
                body,
                span,
            } => {
                let cond_ty = self.check_expr(condition, env, loop_depth)?;
                self.expect_type(&Type::Bool, &cond_ty, Some(*span))?;
                self.check_block(body, env, ret_ty, loop_depth + 1)?;
            }
            Statement::Loop { body, .. } => {
                self.check_block(body, env, ret_ty, loop_depth + 1)?;
            }
            Statement::For {
                name,
                iterable,
                body,
                span,
            } => {
                let iterable_ty = self.check_expr(iterable, env, loop_depth)?;
                let item_ty = match iterable_ty {
                    Type::Array(item_ty, _) | Type::Vec(item_ty) => item_ty,
                    Type::String => Box::new(Type::String),
                    Type::Range => Box::new(Type::I64),
                    _ => {
                        return Err(MiniError::type_error(
                            "`for` expects an array, vec, String, or range",
                            Some(*span),
                        ))
                    }
                };
                env.push(HashMap::new());
                env.last_mut().unwrap().insert(
                    name.clone(),
                    VarInfo {
                        ty: *item_ty,
                        mutable: false,
                    },
                );
                self.check_block(body, env, ret_ty, loop_depth + 1)?;
                env.pop();
            }
        }
        Ok(())
    }

    fn check_expr(
        &self,
        expr: &Expression,
        env: &mut Vec<HashMap<String, VarInfo>>,
        loop_depth: usize,
    ) -> Result<Type> {
        match expr {
            Expression::Int(_, _) => Ok(Type::I64),
            Expression::Float(_, _) => Ok(Type::F64),
            Expression::Bool(_, _) => Ok(Type::Bool),
            Expression::String(_, _) => Ok(Type::String),
            Expression::Range { start, end, span } => {
                let start_ty = self.check_expr(start, env, loop_depth)?;
                let end_ty = self.check_expr(end, env, loop_depth)?;
                self.expect_type(&Type::I64, &start_ty, Some(start.span()))?;
                self.expect_type(&Type::I64, &end_ty, Some(end.span()))?;
                let _ = span;
                Ok(Type::Range)
            }
            Expression::Tuple(items, _) => {
                let mut types = Vec::new();
                for item in items {
                    types.push(self.check_expr(item, env, loop_depth)?);
                }
                if types.iter().any(Type::contains_ref) {
                    return Err(MiniError::type_error(
                        "tuples cannot store references in Rust Mini",
                        Some(expr.span()),
                    ));
                }
                Ok(Type::Tuple(types))
            }
            Expression::Array(items, span) => {
                let Some(first) = items.first() else {
                    return Err(MiniError::type_error(
                        "cannot infer type of empty array",
                        Some(*span),
                    ));
                };
                let elem_ty = self.check_expr(first, env, loop_depth)?;
                for item in items.iter().skip(1) {
                    let item_ty = self.check_expr(item, env, loop_depth)?;
                    self.expect_type(&elem_ty, &item_ty, Some(item.span()))?;
                }
                if elem_ty.contains_ref() {
                    return Err(MiniError::type_error(
                        "arrays cannot store references in Rust Mini",
                        Some(*span),
                    ));
                }
                Ok(Type::Array(Box::new(elem_ty), items.len()))
            }
            Expression::Vec(items, span) => {
                let Some(first) = items.first() else {
                    return Err(MiniError::type_error(
                        "cannot infer type of empty vec",
                        Some(*span),
                    ));
                };
                let elem_ty = self.check_expr(first, env, loop_depth)?;
                for item in items.iter().skip(1) {
                    let item_ty = self.check_expr(item, env, loop_depth)?;
                    self.expect_type(&elem_ty, &item_ty, Some(item.span()))?;
                }
                if elem_ty.contains_ref() {
                    return Err(MiniError::type_error(
                        "vecs cannot store references in Rust Mini",
                        Some(*span),
                    ));
                }
                Ok(Type::Vec(Box::new(elem_ty)))
            }
            Expression::Unit(_) => Ok(Type::Unit),
            Expression::Var(name, span) => lookup(env, name)
                .map(|info| info.ty.clone())
                .ok_or_else(|| {
                    MiniError::type_error(format!("unknown variable `{}`", name), Some(*span))
                }),
            Expression::Unary { op, expr, span } => {
                let ty = self.check_expr(expr, env, loop_depth)?;
                match op {
                    UnaryOp::Neg => {
                        if !matches!(ty, Type::I64 | Type::F64) {
                            return Err(MiniError::type_error(
                                format!("expected numeric type, found `{:?}`", ty),
                                Some(*span),
                            ));
                        }
                        Ok(ty)
                    }
                    UnaryOp::Not => {
                        self.expect_type(&Type::Bool, &ty, Some(*span))?;
                        Ok(Type::Bool)
                    }
                }
            }
            Expression::Binary {
                op,
                left,
                right,
                span,
            } => {
                let lt = self.check_expr(left, env, loop_depth)?;
                let rt = self.check_expr(right, env, loop_depth)?;
                match op {
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Rem => {
                        if !matches!(lt, Type::I64 | Type::F64) {
                            return Err(MiniError::type_error(
                                format!("expected numeric type, found `{:?}`", lt),
                                Some(*span),
                            ));
                        }
                        self.expect_type(&lt, &rt, Some(*span))?;
                        Ok(lt)
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        if !matches!(lt, Type::I64 | Type::F64) {
                            return Err(MiniError::type_error(
                                format!("expected numeric type, found `{:?}`", lt),
                                Some(*span),
                            ));
                        }
                        self.expect_type(&lt, &rt, Some(*span))?;
                        Ok(Type::Bool)
                    }
                    BinaryOp::Eq | BinaryOp::Ne => {
                        self.expect_type(&lt, &rt, Some(*span))?;
                        Ok(Type::Bool)
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        self.expect_type(&Type::Bool, &lt, Some(*span))?;
                        self.expect_type(&Type::Bool, &rt, Some(*span))?;
                        Ok(Type::Bool)
                    }
                }
            }
            Expression::Call { name, args, span } => {
                let name = builtin_alias(name);
                if matches!(
                    name,
                    "logo_forward"
                        | "logo_back"
                        | "logo_right"
                        | "logo_left"
                        | "logo_set_heading"
                        | "logo_circle"
                        | "logo_width"
                ) {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            format!("function `{}` expects 1 argument", name),
                            Some(*span),
                        ));
                    }
                    let arg_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::I64, &arg_ty, Some(args[0].span()))?;
                    return Ok(Type::Unit);
                }
                if matches!(name, "logo_pen_up" | "logo_pen_down" | "logo_clear") {
                    if !args.is_empty() {
                        return Err(MiniError::type_error(
                            format!("function `{}` expects 0 arguments", name),
                            Some(*span),
                        ));
                    }
                    return Ok(Type::Unit);
                }
                if name == "logo_home" {
                    if !args.is_empty() {
                        return Err(MiniError::type_error(
                            "function `logo_home` expects 0 arguments",
                            Some(*span),
                        ));
                    }
                    return Ok(Type::Unit);
                }
                if name == "logo_heading" {
                    if !args.is_empty() {
                        return Err(MiniError::type_error(
                            "function `logo_heading` expects 0 arguments",
                            Some(*span),
                        ));
                    }
                    return Ok(Type::I64);
                }
                if name == "logo_set_position" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "function `logo_set_position` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    let x_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let y_ty = self.check_expr(&args[1], env, loop_depth)?;
                    self.expect_type(&Type::I64, &x_ty, Some(args[0].span()))?;
                    self.expect_type(&Type::I64, &y_ty, Some(args[1].span()))?;
                    return Ok(Type::Unit);
                }
                if matches!(name, "logo_pen_color" | "logo_save") {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            format!("function `{}` expects 1 argument", name),
                            Some(*span),
                        ));
                    }
                    let arg_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &arg_ty, Some(args[0].span()))?;
                    return Ok(Type::Unit);
                }
                if name == "logo_background" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "function `logo_background` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    let arg_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &arg_ty, Some(args[0].span()))?;
                    return Ok(Type::Unit);
                }
                if name == "logo_save_with_size" {
                    if args.len() != 3 {
                        return Err(MiniError::type_error(
                            "function `logo_save_with_size` expects 3 arguments",
                            Some(*span),
                        ));
                    }
                    let path_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let width_ty = self.check_expr(&args[1], env, loop_depth)?;
                    let height_ty = self.check_expr(&args[2], env, loop_depth)?;
                    self.expect_type(&Type::String, &path_ty, Some(args[0].span()))?;
                    self.expect_type(&Type::I64, &width_ty, Some(args[1].span()))?;
                    self.expect_type(&Type::I64, &height_ty, Some(args[2].span()))?;
                    return Ok(Type::Unit);
                }
                if name == "args" {
                    if !args.is_empty() {
                        return Err(MiniError::type_error(
                            "function `args` expects 0 arguments",
                            Some(*span),
                        ));
                    }
                    return Ok(Type::Vec(Box::new(Type::String)));
                }
                if name == "clock_ms" {
                    if !args.is_empty() {
                        return Err(MiniError::type_error(
                            "function `clock_ms` expects 0 arguments",
                            Some(*span),
                        ));
                    }
                    return Ok(Type::I64);
                }
                if name == "clear" {
                    if !args.is_empty() {
                        return Err(MiniError::type_error(
                            "function `clear` expects 0 arguments",
                            Some(*span),
                        ));
                    }
                    return Ok(Type::Unit);
                }
                if name == "read_key" {
                    if !args.is_empty() {
                        return Err(MiniError::type_error(
                            "function `read_key` expects 0 arguments",
                            Some(*span),
                        ));
                    }
                    return Ok(Type::String);
                }
                if name == "sleep_ms" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "function `sleep_ms` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    let ms_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::I64, &ms_ty, Some(args[0].span()))?;
                    return Ok(Type::Unit);
                }
                if name == "rand_i64" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "function `rand_i64` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    let min_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let max_ty = self.check_expr(&args[1], env, loop_depth)?;
                    self.expect_type(&Type::I64, &min_ty, Some(args[0].span()))?;
                    self.expect_type(&Type::I64, &max_ty, Some(args[1].span()))?;
                    return Ok(Type::I64);
                }
                if name == "color" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "function `color` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    let text_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let name_ty = self.check_expr(&args[1], env, loop_depth)?;
                    self.expect_type(&Type::String, &text_ty, Some(args[0].span()))?;
                    self.expect_type(&Type::String, &name_ty, Some(args[1].span()))?;
                    return Ok(Type::String);
                }
                if name == "input" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "function `input` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    let prompt_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &prompt_ty, Some(args[0].span()))?;
                    return Ok(Type::String);
                }
                if name == "read_i64_alias" || name == "read_f64_alias" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            format!("function `{}` expects 2 arguments", name),
                            Some(*span),
                        ));
                    }
                    let prompt_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &prompt_ty, Some(args[0].span()))?;
                    if name == "read_i64_alias" {
                        let default_ty = self.check_expr(&args[1], env, loop_depth)?;
                        self.expect_type(&Type::I64, &default_ty, Some(args[1].span()))?;
                        return Ok(Type::I64);
                    }
                    let default_ty = self.check_expr(&args[1], env, loop_depth)?;
                    self.expect_type(&Type::F64, &default_ty, Some(args[1].span()))?;
                    return Ok(Type::F64);
                }
                if name == "parse_f64" || name == "parse_i64" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            format!("function `{}` expects 1 argument", name),
                            Some(*span),
                        ));
                    }
                    let text_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &text_ty, Some(args[0].span()))?;
                    return Ok(Type::Result(
                        Box::new(if name == "parse_f64" {
                            Type::F64
                        } else {
                            Type::I64
                        }),
                        Box::new(Type::String),
                    ));
                }
                if name == "unwrap_or" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "function `unwrap_or` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    let wrapped_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let default_ty = self.check_expr(&args[1], env, loop_depth)?;
                    let inner_ty = match wrapped_ty {
                        Type::Option(inner) => *inner,
                        Type::Result(ok, _) => *ok,
                        other => {
                            return Err(MiniError::type_error(
                                format!(
                                    "function `unwrap_or` expects Option or Result, found `{:?}`",
                                    other
                                ),
                                Some(args[0].span()),
                            ));
                        }
                    };
                    self.expect_type(&inner_ty, &default_ty, Some(args[1].span()))?;
                    return Ok(inner_ty);
                }
                if name == "len" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "function `len` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    match self.check_expr(&args[0], env, loop_depth)? {
                        Type::String | Type::Array(_, _) | Type::Vec(_) => return Ok(Type::I64),
                        other => {
                            return Err(MiniError::type_error(
                                format!(
                                    "function `len` expects String, array, or vec, found `{:?}`",
                                    other
                                ),
                                Some(args[0].span()),
                            ));
                        }
                    }
                }
                if name == "concat" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "function `concat` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    let left_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let right_ty = self.check_expr(&args[1], env, loop_depth)?;
                    self.expect_type(&Type::String, &left_ty, Some(args[0].span()))?;
                    self.expect_type(&Type::String, &right_ty, Some(args[1].span()))?;
                    return Ok(Type::String);
                }
                if name == "contains" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "function `contains` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    let haystack_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let needle_ty = self.check_expr(&args[1], env, loop_depth)?;
                    self.expect_type(&Type::String, &haystack_ty, Some(args[0].span()))?;
                    self.expect_type(&Type::String, &needle_ty, Some(args[1].span()))?;
                    return Ok(Type::Bool);
                }
                if name == "__format_macro" || name == "__print_macro" || name == "__println_macro"
                {
                    if args.is_empty() {
                        return Err(MiniError::type_error(
                            "formatting macro expects a format string",
                            Some(*span),
                        ));
                    }
                    let format_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &format_ty, Some(args[0].span()))?;
                    for arg in args.iter().skip(1) {
                        self.check_expr(arg, env, loop_depth)?;
                    }
                    return Ok(if name == "__format_macro" {
                        Type::String
                    } else {
                        Type::Unit
                    });
                }
                if name == "env" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "function `env` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    let name_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &name_ty, Some(args[0].span()))?;
                    return Ok(Type::String);
                }
                if name == "read_file" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "function `read_file` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    let path_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &path_ty, Some(args[0].span()))?;
                    return Ok(Type::Result(Box::new(Type::String), Box::new(Type::String)));
                }
                if name == "write_file" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "function `write_file` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    let path_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let body_ty = self.check_expr(&args[1], env, loop_depth)?;
                    self.expect_type(&Type::String, &path_ty, Some(args[0].span()))?;
                    self.expect_type(&Type::String, &body_ty, Some(args[1].span()))?;
                    return Ok(Type::Unit);
                }
                if name == "push" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "function `push` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    let target_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let value_ty = self.check_expr(&args[1], env, loop_depth)?;
                    match target_ty {
                        Type::MutRef(inner) => {
                            match *inner {
                                Type::Vec(elem) => {
                                    self.expect_type(&elem, &value_ty, Some(args[1].span()))?
                                }
                                other => {
                                    return Err(MiniError::type_error(
                                    format!("function `push` expects `&mut Vec<T>`, found `&mut {:?}`", other),
                                    Some(args[0].span()),
                                ));
                                }
                            }
                        }
                        other => {
                            return Err(MiniError::type_error(
                                format!(
                                    "function `push` expects mutable vec reference, found `{:?}`",
                                    other
                                ),
                                Some(args[0].span()),
                            ));
                        }
                    }
                    return Ok(Type::Unit);
                }
                if name == "print" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "function `print` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    self.check_expr(&args[0], env, loop_depth)?;
                    return Ok(Type::Unit);
                }
                let sig = self.functions.get(name).ok_or_else(|| {
                    MiniError::type_error(format!("unknown function `{}`", name), Some(*span))
                })?;
                if sig.params.len() != args.len() {
                    return Err(MiniError::type_error(
                        format!("function `{}` expects {} arguments", name, sig.params.len()),
                        Some(*span),
                    ));
                }
                for (arg, expected) in args.iter().zip(&sig.params) {
                    let actual = self.check_expr(arg, env, loop_depth)?;
                    self.expect_type(expected, &actual, Some(arg.span()))?;
                }
                Ok(sig.ret.clone())
            }
            Expression::MethodCall {
                receiver,
                name,
                args,
                span,
            } => {
                let recv_ty = self.check_expr(receiver, env, loop_depth)?;
                if name == "len" && args.is_empty() {
                    match recv_ty {
                        Type::String | Type::Array(_, _) | Type::Vec(_) => return Ok(Type::I64),
                        other => {
                            return Err(MiniError::type_error(
                                format!("method `len` cannot be called on `{:?}`", other),
                                Some(*span),
                            ))
                        }
                    }
                }
                if name == "trim" && args.is_empty() {
                    self.expect_type(&Type::String, &recv_ty, Some(receiver.span()))?;
                    return Ok(Type::String);
                }
                if matches!(name.as_str(), "to_lowercase" | "to_uppercase") && args.is_empty() {
                    self.expect_type(&Type::String, &recv_ty, Some(receiver.span()))?;
                    return Ok(Type::String);
                }
                if matches!(name.as_str(), "contains" | "starts_with" | "ends_with") {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            format!("method `{}` expects 1 argument", name),
                            Some(*span),
                        ));
                    }
                    self.expect_type(&Type::String, &recv_ty, Some(receiver.span()))?;
                    let arg_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &arg_ty, Some(args[0].span()))?;
                    return Ok(Type::Bool);
                }
                if name == "replace" {
                    if args.len() != 2 {
                        return Err(MiniError::type_error(
                            "method `replace` expects 2 arguments",
                            Some(*span),
                        ));
                    }
                    self.expect_type(&Type::String, &recv_ty, Some(receiver.span()))?;
                    let from_ty = self.check_expr(&args[0], env, loop_depth)?;
                    let to_ty = self.check_expr(&args[1], env, loop_depth)?;
                    self.expect_type(&Type::String, &from_ty, Some(args[0].span()))?;
                    self.expect_type(&Type::String, &to_ty, Some(args[1].span()))?;
                    return Ok(Type::String);
                }
                if name == "push_str" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "method `push_str` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    self.expect_type(&Type::String, &recv_ty, Some(receiver.span()))?;
                    let arg_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&Type::String, &arg_ty, Some(args[0].span()))?;
                    return Ok(Type::Unit);
                }
                if name == "push" {
                    if args.len() != 1 {
                        return Err(MiniError::type_error(
                            "method `push` expects 1 argument",
                            Some(*span),
                        ));
                    }
                    let Type::Vec(item_ty) = recv_ty else {
                        return Err(MiniError::type_error(
                            "method `push` expects Vec receiver",
                            Some(receiver.span()),
                        ));
                    };
                    let arg_ty = self.check_expr(&args[0], env, loop_depth)?;
                    self.expect_type(&item_ty, &arg_ty, Some(args[0].span()))?;
                    return Ok(Type::Unit);
                }
                if name == "pop" && args.is_empty() {
                    let Type::Vec(item_ty) = recv_ty else {
                        return Err(MiniError::type_error(
                            "method `pop` expects Vec receiver",
                            Some(receiver.span()),
                        ));
                    };
                    return Ok(Type::Option(item_ty));
                }
                if matches!(name.as_str(), "is_some" | "is_none") && args.is_empty() {
                    let Type::Option(_) = recv_ty else {
                        return Err(MiniError::type_error(
                            format!("method `{}` expects Option receiver", name),
                            Some(receiver.span()),
                        ));
                    };
                    return Ok(Type::Bool);
                }
                if matches!(name.as_str(), "is_ok" | "is_err") && args.is_empty() {
                    let Type::Result(_, _) = recv_ty else {
                        return Err(MiniError::type_error(
                            format!("method `{}` expects Result receiver", name),
                            Some(receiver.span()),
                        ));
                    };
                    return Ok(Type::Bool);
                }
                if name == "unwrap_or" {
                    if matches!(recv_ty, Type::Option(_) | Type::Result(_, _)) {
                        if args.len() != 1 {
                            return Err(MiniError::type_error(
                                "method `unwrap_or` expects 1 argument",
                                Some(*span),
                            ));
                        }
                        let inner_ty = match recv_ty.clone() {
                            Type::Option(inner) => inner,
                            Type::Result(ok, _) => ok,
                            _ => unreachable!(),
                        };
                        let default_ty = self.check_expr(&args[0], env, loop_depth)?;
                        self.expect_type(&inner_ty, &default_ty, Some(args[0].span()))?;
                        return Ok(*inner_ty);
                    }
                }
                let type_name = match &recv_ty {
                    Type::Struct(name) | Type::Enum(name) => name.clone(),
                    other => {
                        return Err(MiniError::type_error(
                            format!("cannot call method `{}` on `{:?}`", name, other),
                            Some(*span),
                        ));
                    }
                };
                let full_name = format!("{}::{}", type_name, name);
                let sig = self.functions.get(&full_name).ok_or_else(|| {
                    MiniError::type_error(
                        format!("unknown method `{}` for `{}`", name, type_name),
                        Some(*span),
                    )
                })?;
                if sig.params.len() != args.len() + 1 {
                    return Err(MiniError::type_error(
                        format!(
                            "method `{}` expects {} arguments",
                            name,
                            sig.params.len() - 1
                        ),
                        Some(*span),
                    ));
                }
                match &sig.params[0] {
                    Type::Ref(inner) | Type::MutRef(inner) => {
                        self.expect_type(inner, &recv_ty, Some(receiver.span()))?;
                    }
                    expected => self.expect_type(expected, &recv_ty, Some(receiver.span()))?,
                }
                for (arg, expected) in args.iter().zip(sig.params.iter().skip(1)) {
                    let actual = self.check_expr(arg, env, loop_depth)?;
                    self.expect_type(expected, &actual, Some(arg.span()))?;
                }
                Ok(sig.ret.clone())
            }
            Expression::StructLiteral { name, fields, span } => {
                let def_fields = self.structs.get(name).ok_or_else(|| {
                    MiniError::type_error(format!("unknown struct `{}`", name), Some(*span))
                })?;
                if def_fields.len() != fields.len() {
                    return Err(MiniError::type_error(
                        format!("struct `{}` expects {} fields", name, def_fields.len()),
                        Some(*span),
                    ));
                }
                for field in def_fields {
                    let Some((_, value)) = fields.iter().find(|(given, _)| given == &field.name)
                    else {
                        return Err(MiniError::type_error(
                            format!("missing field `{}` for struct `{}`", field.name, name),
                            Some(*span),
                        ));
                    };
                    let actual = self.check_expr(value, env, loop_depth)?;
                    let field_ty = self.resolve_type(&field.ty);
                    self.expect_type(&field_ty, &actual, Some(value.span()))?;
                }
                for (given, _) in fields {
                    if !def_fields.iter().any(|field| field.name == *given) {
                        return Err(MiniError::type_error(
                            format!("unknown field `{}` for struct `{}`", given, name),
                            Some(*span),
                        ));
                    }
                }
                Ok(Type::Struct(name.clone()))
            }
            Expression::EnumLiteral {
                enum_name,
                variant,
                value,
                span,
            } => {
                if enum_name == "Option" {
                    return match (variant.as_str(), value) {
                        ("Some", Some(actual)) => {
                            let actual_ty = self.check_expr(actual, env, loop_depth)?;
                            Ok(Type::Option(Box::new(actual_ty)))
                        }
                        ("None", None) => Ok(Type::Option(Box::new(Type::Unit))),
                        ("Some", None) => Err(MiniError::type_error(
                            "variant `Option::Some` expects payload",
                            Some(*span),
                        )),
                        ("None", Some(_)) => Err(MiniError::type_error(
                            "variant `Option::None` has no payload",
                            Some(*span),
                        )),
                        _ => Err(MiniError::type_error(
                            format!("unknown variant `{}` for enum `Option`", variant),
                            Some(*span),
                        )),
                    };
                }
                if enum_name == "Result" {
                    return match (variant.as_str(), value) {
                        ("Ok", Some(actual)) => {
                            let actual_ty = self.check_expr(actual, env, loop_depth)?;
                            Ok(Type::Result(Box::new(actual_ty), Box::new(Type::String)))
                        }
                        ("Err", Some(actual)) => {
                            let actual_ty = self.check_expr(actual, env, loop_depth)?;
                            Ok(Type::Result(Box::new(Type::Unit), Box::new(actual_ty)))
                        }
                        ("Ok", None) => Err(MiniError::type_error(
                            "variant `Result::Ok` expects payload",
                            Some(*span),
                        )),
                        ("Err", None) => Err(MiniError::type_error(
                            "variant `Result::Err` expects payload",
                            Some(*span),
                        )),
                        _ => Err(MiniError::type_error(
                            format!("unknown variant `{}` for enum `Result`", variant),
                            Some(*span),
                        )),
                    };
                }
                let variants = self.enums.get(enum_name).ok_or_else(|| {
                    MiniError::type_error(format!("unknown enum `{}`", enum_name), Some(*span))
                })?;
                let def = variants
                    .iter()
                    .find(|candidate| candidate.name == *variant)
                    .ok_or_else(|| {
                        MiniError::type_error(
                            format!("unknown variant `{}` for enum `{}`", variant, enum_name),
                            Some(*span),
                        )
                    })?;
                match (&def.payload, value) {
                    (Some(expected), Some(actual)) => {
                        let expected = self.resolve_type(expected);
                        let actual_ty = self.check_expr(actual, env, loop_depth)?;
                        self.expect_type(&expected, &actual_ty, Some(actual.span()))?;
                    }
                    (None, None) => {}
                    (Some(_), None) => {
                        return Err(MiniError::type_error(
                            format!("variant `{}::{}` expects payload", enum_name, variant),
                            Some(*span),
                        ));
                    }
                    (None, Some(_)) => {
                        return Err(MiniError::type_error(
                            format!("variant `{}::{}` has no payload", enum_name, variant),
                            Some(*span),
                        ));
                    }
                }
                Ok(Type::Enum(enum_name.clone()))
            }
            Expression::Index {
                target,
                index,
                span,
            } => {
                let target_ty = self.check_expr(target, env, loop_depth)?;
                let index_ty = self.check_expr(index, env, loop_depth)?;
                self.expect_type(&Type::I64, &index_ty, Some(index.span()))?;
                match target_ty {
                    Type::Array(inner, _) => Ok(*inner),
                    Type::Vec(inner) => Ok(*inner),
                    other => Err(MiniError::type_error(
                        format!("cannot index `{:?}`", other),
                        Some(*span),
                    )),
                }
            }
            Expression::Field {
                target,
                field,
                span,
            } => {
                let target_ty = self.check_expr(target, env, loop_depth)?;
                let target_ty = match target_ty {
                    Type::Ref(inner) | Type::MutRef(inner) => *inner,
                    other => other,
                };
                match target_ty {
                    Type::Tuple(items) => {
                        let index = field.parse::<usize>().map_err(|_| {
                            MiniError::type_error("tuple field must be numeric", Some(*span))
                        })?;
                        items.get(index).cloned().ok_or_else(|| {
                            MiniError::type_error(
                                format!("tuple field `{}` out of range", field),
                                Some(*span),
                            )
                        })
                    }
                    Type::Struct(name) => {
                        let fields = self.structs.get(&name).ok_or_else(|| {
                            MiniError::type_error(format!("unknown struct `{}`", name), Some(*span))
                        })?;
                        fields
                            .iter()
                            .find(|def| def.name == *field)
                            .map(|def| self.resolve_type(&def.ty))
                            .ok_or_else(|| {
                                MiniError::type_error(
                                    format!("unknown field `{}` for struct `{}`", field, name),
                                    Some(*span),
                                )
                            })
                    }
                    other => Err(MiniError::type_error(
                        format!("cannot access field `{}` on `{:?}`", field, other),
                        Some(*span),
                    )),
                }
            }
            Expression::Match { value, arms, span } => {
                let value_ty = self.check_expr(value, env, loop_depth)?;
                let (enum_name, variants) = match value_ty {
                    Type::Enum(enum_name) => {
                        let variants = self.enums.get(&enum_name).ok_or_else(|| {
                            MiniError::type_error(
                                format!("unknown enum `{}`", enum_name),
                                Some(*span),
                            )
                        })?;
                        (enum_name, variants.clone())
                    }
                    Type::Option(inner) => (
                        "Option".to_string(),
                        vec![
                            EnumVariant {
                                name: "Some".to_string(),
                                payload: Some(*inner),
                            },
                            EnumVariant {
                                name: "None".to_string(),
                                payload: None,
                            },
                        ],
                    ),
                    Type::Result(ok, err) => (
                        "Result".to_string(),
                        vec![
                            EnumVariant {
                                name: "Ok".to_string(),
                                payload: Some(*ok),
                            },
                            EnumVariant {
                                name: "Err".to_string(),
                                payload: Some(*err),
                            },
                        ],
                    ),
                    _ => {
                        return Err(MiniError::type_error(
                            "match expects enum, Option, or Result value",
                            Some(*span),
                        ));
                    }
                };
                let mut result_ty = None;
                let mut has_wildcard = false;
                let mut covered = Vec::new();
                for arm in arms {
                    env.push(HashMap::new());
                    match &arm.pattern {
                        Pattern::Wildcard => has_wildcard = true,
                        Pattern::EnumVariant {
                            enum_name: pat_enum,
                            variant,
                            binding,
                        } => {
                            if pat_enum != &enum_name {
                                return Err(MiniError::type_error(
                                    format!(
                                        "expected enum `{}`, found pattern for `{}`",
                                        enum_name, pat_enum
                                    ),
                                    Some(arm.span),
                                ));
                            }
                            let def = variants
                                .iter()
                                .find(|candidate| candidate.name == *variant)
                                .ok_or_else(|| {
                                    MiniError::type_error(
                                        format!(
                                            "unknown variant `{}` for enum `{}`",
                                            variant, enum_name
                                        ),
                                        Some(arm.span),
                                    )
                                })?;
                            covered.push(variant.clone());
                            match (&def.payload, binding) {
                                (Some(payload), Some(name)) => {
                                    env.last_mut().unwrap().insert(
                                        name.clone(),
                                        VarInfo {
                                            ty: payload.clone(),
                                            mutable: false,
                                        },
                                    );
                                }
                                (Some(_), None) => {
                                    return Err(MiniError::type_error(
                                        format!(
                                            "variant `{}::{}` payload must be bound",
                                            enum_name, variant
                                        ),
                                        Some(arm.span),
                                    ));
                                }
                                (None, Some(_)) => {
                                    return Err(MiniError::type_error(
                                        format!(
                                            "variant `{}::{}` has no payload",
                                            enum_name, variant
                                        ),
                                        Some(arm.span),
                                    ));
                                }
                                (None, None) => {}
                            }
                        }
                    }
                    let arm_ty = self.check_expr(&arm.body, env, loop_depth)?;
                    env.pop();
                    if let Some(expected) = &result_ty {
                        self.expect_type(expected, &arm_ty, Some(arm.span))?;
                    } else {
                        result_ty = Some(arm_ty);
                    }
                }
                if !has_wildcard
                    && variants
                        .iter()
                        .any(|variant| !covered.contains(&variant.name))
                {
                    return Err(MiniError::type_error("non-exhaustive match", Some(*span)));
                }
                Ok(result_ty.unwrap_or(Type::Unit))
            }
            Expression::Block(block) => self.check_block(block, env, &Type::Unit, loop_depth),
            Expression::If {
                condition,
                then_block,
                else_block,
                span,
            } => {
                let cond = self.check_expr(condition, env, loop_depth)?;
                self.expect_type(&Type::Bool, &cond, Some(*span))?;
                let then_ty = self.check_block(then_block, env, &Type::Unit, loop_depth)?;
                let else_ty = if let Some(else_block) = else_block {
                    self.check_block(else_block, env, &Type::Unit, loop_depth)?
                } else {
                    Type::Unit
                };
                self.expect_type(&then_ty, &else_ty, Some(*span))?;
                Ok(then_ty)
            }
            Expression::Ref {
                mutable,
                expr,
                span,
            } => match &**expr {
                Expression::Var(name, _) => {
                    let info = lookup(env, name).ok_or_else(|| {
                        MiniError::type_error(format!("unknown variable `{}`", name), Some(*span))
                    })?;
                    if *mutable && !info.mutable {
                        return Err(MiniError::type_error(
                            format!(
                                "cannot take mutable reference to immutable variable `{}`",
                                name
                            ),
                            Some(*span),
                        ));
                    }
                    if *mutable {
                        Ok(Type::MutRef(Box::new(info.ty.clone())))
                    } else {
                        Ok(Type::Ref(Box::new(info.ty.clone())))
                    }
                }
                _ => Err(MiniError::type_error(
                    "reference target must be a variable",
                    Some(*span),
                )),
            },
            Expression::Deref { expr, span } => match self.check_expr(expr, env, loop_depth)? {
                Type::Ref(inner) | Type::MutRef(inner) => Ok(*inner),
                other => Err(MiniError::type_error(
                    format!("cannot dereference `{:?}`", other),
                    Some(*span),
                )),
            },
        }
    }

    fn expect_type(&self, expected: &Type, actual: &Type, span: Option<Span>) -> Result<()> {
        if expected == actual
            || (*expected == Type::String && *actual == Type::Str)
            || option_types_compatible(expected, actual)
            || result_types_compatible(expected, actual)
        {
            Ok(())
        } else {
            Err(MiniError::type_error(
                format!("expected `{:?}`, found `{:?}`", expected, actual),
                span,
            ))
        }
    }

    fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Struct(name) if self.enums.contains_key(name) => Type::Enum(name.clone()),
            Type::Range => Type::Range,
            Type::Tuple(items) => {
                Type::Tuple(items.iter().map(|item| self.resolve_type(item)).collect())
            }
            Type::Array(item, len) => Type::Array(Box::new(self.resolve_type(item)), *len),
            Type::Vec(item) => Type::Vec(Box::new(self.resolve_type(item))),
            Type::Option(item) => Type::Option(Box::new(self.resolve_type(item))),
            Type::Result(ok, err) => Type::Result(
                Box::new(self.resolve_type(ok)),
                Box::new(self.resolve_type(err)),
            ),
            Type::Ref(item) => Type::Ref(Box::new(self.resolve_type(item))),
            Type::MutRef(item) => Type::MutRef(Box::new(self.resolve_type(item))),
            other => other.clone(),
        }
    }
}

fn option_types_compatible(expected: &Type, actual: &Type) -> bool {
    match (expected, actual) {
        (Type::Option(expected), Type::Option(actual)) => {
            **actual == Type::Unit || **expected == **actual
        }
        _ => false,
    }
}

fn result_types_compatible(expected: &Type, actual: &Type) -> bool {
    match (expected, actual) {
        (Type::Result(expected_ok, expected_err), Type::Result(actual_ok, actual_err)) => {
            (**actual_ok == Type::Unit || **expected_ok == **actual_ok)
                && (**actual_err == Type::Unit || **expected_err == **actual_err)
        }
        _ => false,
    }
}

fn lookup<'a>(env: &'a [HashMap<String, VarInfo>], name: &str) -> Option<&'a VarInfo> {
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

fn replace_self_type(ty: &Type, target: &str) -> Type {
    match ty {
        Type::Struct(name) if name == "Self" => Type::Struct(target.to_string()),
        Type::Range => Type::Range,
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| replace_self_type(item, target))
                .collect(),
        ),
        Type::Array(item, len) => Type::Array(Box::new(replace_self_type(item, target)), *len),
        Type::Vec(item) => Type::Vec(Box::new(replace_self_type(item, target))),
        Type::Option(item) => Type::Option(Box::new(replace_self_type(item, target))),
        Type::Result(ok, err) => Type::Result(
            Box::new(replace_self_type(ok, target)),
            Box::new(replace_self_type(err, target)),
        ),
        Type::Ref(item) => Type::Ref(Box::new(replace_self_type(item, target))),
        Type::MutRef(item) => Type::MutRef(Box::new(replace_self_type(item, target))),
        other => other.clone(),
    }
}

fn block_guaranteed_returns(block: &Block) -> bool {
    block.statements.iter().any(|stmt| match stmt {
        Statement::Return { .. } => true,
        Statement::Expr(Expression::If {
            then_block,
            else_block: Some(else_block),
            ..
        }) => block_guaranteed_returns(then_block) && block_guaranteed_returns(else_block),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use crate::{check_program, parse_source};

    #[test]
    fn checks_reference_parameter() {
        let p = parse_source(
            "fn main(){ let mut x:i64=1; inc(&mut x); } fn inc(x:&mut i64){ *x = *x + 1; }",
        )
        .unwrap();
        check_program(&p).unwrap();
    }
}
