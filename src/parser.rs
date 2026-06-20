use crate::ast::*;
use crate::error::{MiniError, Result, Span};
use crate::token::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    lifetime_scope: Vec<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            lifetime_scope: Vec::new(),
        }
    }

    pub fn parse_program(&mut self) -> Result<Program> {
        let mut modules = Vec::new();
        let mut uses = Vec::new();
        let mut traits = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut impls = Vec::new();
        let mut functions = Vec::new();
        while !self.at(&TokenKind::Eof) {
            let public = self.eat(&TokenKind::Pub);
            if self.at(&TokenKind::Mod) {
                modules.push(self.parse_module_import()?);
            } else if self.at(&TokenKind::Use) {
                uses.push(self.parse_use_import()?);
            } else if self.at(&TokenKind::Trait) {
                traits.push(self.parse_trait(public)?);
            } else if self.at(&TokenKind::Struct) {
                structs.push(self.parse_struct(public)?);
            } else if self.at(&TokenKind::Enum) {
                enums.push(self.parse_enum(public)?);
            } else if self.at(&TokenKind::Impl) {
                impls.push(self.parse_impl(public)?);
            } else {
                functions.push(self.parse_function(public)?);
            }
        }
        Ok(Program {
            modules,
            uses,
            traits,
            structs,
            enums,
            impls,
            functions,
        })
    }

    fn parse_module_import(&mut self) -> Result<ModuleImport> {
        let span = self.expect_simple(TokenKind::Mod, "expected `mod`")?;
        let path = match self.current().kind.clone() {
            TokenKind::String(path) => {
                self.advance();
                path
            }
            TokenKind::Ident(path) => {
                self.advance();
                path
            }
            _ => {
                return Err(MiniError::parse(
                    "expected module name or string path after `mod`",
                    self.current().span,
                ))
            }
        };
        self.expect_simple(TokenKind::Semi, "expected `;` after module import")?;
        Ok(ModuleImport { path, span })
    }

    fn parse_use_import(&mut self) -> Result<UseImport> {
        let span = self.expect_simple(TokenKind::Use, "expected `use`")?;
        let mut path = Vec::new();
        path.push(self.expect_ident("expected path after `use`")?);
        let mut glob = false;
        while self.eat(&TokenKind::ColonColon) {
            if self.eat(&TokenKind::Star) {
                glob = true;
                break;
            }
            path.push(self.expect_ident("expected path segment after `::`")?);
        }
        self.expect_simple(TokenKind::Semi, "expected `;` after use import")?;
        Ok(UseImport { path, glob, span })
    }

    fn parse_impl(&mut self, public: bool) -> Result<ImplBlock> {
        let span = self.expect_simple(TokenKind::Impl, "expected `impl`")?;
        let first = self.expect_ident("expected type or trait name after `impl`")?;
        let (trait_name, target) = if self.eat(&TokenKind::For) {
            (
                Some(first),
                self.expect_ident("expected type name after `for`")?,
            )
        } else {
            (None, first)
        };
        self.expect_simple(TokenKind::LBrace, "expected `{` after impl target")?;
        let mut methods = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(MiniError::parse(
                    "expected `}` before end of impl block",
                    self.current().span,
                ));
            }
            methods.push(self.parse_method_function(&target)?);
        }
        self.expect_simple(TokenKind::RBrace, "expected `}` after impl block")?;
        Ok(ImplBlock {
            target,
            trait_name,
            methods,
            public,
            span,
        })
    }

    fn parse_trait(&mut self, public: bool) -> Result<TraitDef> {
        let span = self.expect_simple(TokenKind::Trait, "expected `trait`")?;
        let name = self.expect_ident("expected trait name")?;
        self.expect_simple(TokenKind::LBrace, "expected `{` after trait name")?;
        let mut methods = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(MiniError::parse(
                    "expected `}` before end of trait block",
                    self.current().span,
                ));
            }
            methods.push(self.parse_trait_method()?);
        }
        self.expect_simple(TokenKind::RBrace, "expected `}` after trait block")?;
        Ok(TraitDef {
            name,
            methods,
            public,
            span,
        })
    }

    fn parse_trait_method(&mut self) -> Result<TraitMethod> {
        let span = self.expect_simple(TokenKind::Fn, "expected `fn` in trait block")?;
        let name = self.expect_ident("expected trait method name")?;
        let lifetime_params = self.parse_lifetime_params()?;
        let previous_lifetimes = self.enter_lifetime_scope(&lifetime_params);
        self.expect_simple(TokenKind::LParen, "expected `(` after trait method name")?;
        let params = self.parse_parameter_list("Self")?;
        self.expect_simple(
            TokenKind::RParen,
            "expected `)` after trait method parameters",
        )?;
        let ret_type = if self.eat(&TokenKind::Arrow) {
            self.parse_type()?
        } else {
            Type::Unit
        };
        self.lifetime_scope = previous_lifetimes;
        self.expect_simple(TokenKind::Semi, "expected `;` after trait method")?;
        Ok(TraitMethod {
            name,
            lifetime_params,
            params,
            ret_type,
            span,
        })
    }

    fn parse_struct(&mut self, public: bool) -> Result<StructDef> {
        let span = self.expect_simple(TokenKind::Struct, "expected `struct`")?;
        let name = self.expect_ident("expected struct name")?;
        self.expect_simple(TokenKind::LBrace, "expected `{` after struct name")?;
        let mut fields = Vec::new();
        if !self.at(&TokenKind::RBrace) {
            loop {
                let field_name = self.expect_ident("expected field name")?;
                self.expect_simple(TokenKind::Colon, "expected `:` after field name")?;
                let ty = self.parse_type()?;
                fields.push(StructField {
                    name: field_name,
                    ty,
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
                if self.at(&TokenKind::RBrace) {
                    break;
                }
            }
        }
        self.expect_simple(TokenKind::RBrace, "expected `}` after struct fields")?;
        Ok(StructDef {
            name,
            fields,
            public,
            span,
        })
    }

    fn parse_enum(&mut self, public: bool) -> Result<EnumDef> {
        let span = self.expect_simple(TokenKind::Enum, "expected `enum`")?;
        let name = self.expect_ident("expected enum name")?;
        self.expect_simple(TokenKind::LBrace, "expected `{` after enum name")?;
        let mut variants = Vec::new();
        if !self.at(&TokenKind::RBrace) {
            loop {
                let variant = self.expect_ident("expected enum variant")?;
                let payload = if self.eat(&TokenKind::LParen) {
                    let ty = self.parse_type()?;
                    self.expect_simple(TokenKind::RParen, "expected `)` after enum payload")?;
                    Some(ty)
                } else {
                    None
                };
                variants.push(EnumVariant {
                    name: variant,
                    payload,
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
                if self.at(&TokenKind::RBrace) {
                    break;
                }
            }
        }
        self.expect_simple(TokenKind::RBrace, "expected `}` after enum variants")?;
        Ok(EnumDef {
            name,
            variants,
            public,
            span,
        })
    }

    fn parse_function(&mut self, public: bool) -> Result<Function> {
        let span = self.expect_simple(TokenKind::Fn, "expected `fn`")?;
        let name = self.expect_ident("expected function name")?;
        let lifetime_params = self.parse_lifetime_params()?;
        let previous_lifetimes = self.enter_lifetime_scope(&lifetime_params);
        self.expect_simple(TokenKind::LParen, "expected `(` after function name")?;
        let params = self.parse_parameter_list("Self")?;
        self.expect_simple(TokenKind::RParen, "expected `)` after parameters")?;
        let ret_type = if self.eat(&TokenKind::Arrow) {
            self.parse_type()?
        } else {
            Type::Unit
        };
        self.lifetime_scope = previous_lifetimes;
        let body = self.parse_block()?;
        Ok(Function {
            name,
            lifetime_params,
            params,
            ret_type,
            body,
            public,
            span,
        })
    }

    fn parse_method_function(&mut self, target: &str) -> Result<Function> {
        let span = self.expect_simple(TokenKind::Fn, "expected `fn` in impl block")?;
        let short_name = self.expect_ident("expected method name")?;
        let name = format!("{}::{}", target, short_name);
        let lifetime_params = self.parse_lifetime_params()?;
        let previous_lifetimes = self.enter_lifetime_scope(&lifetime_params);
        self.expect_simple(TokenKind::LParen, "expected `(` after method name")?;
        let mut params = Vec::new();
        if !self.at(&TokenKind::RParen) {
            if self.eat(&TokenKind::Amp) {
                let _lifetime = self.parse_optional_lifetime()?;
                let mutable = self.eat(&TokenKind::Mut);
                if !self.current_is_ident("self") {
                    return Err(MiniError::parse(
                        "expected `self` after receiver `&`",
                        self.current().span,
                    ));
                }
                self.advance();
                let inner = Type::Struct(target.to_string());
                params.push(Parameter {
                    name: "self".to_string(),
                    ty: if mutable {
                        Type::MutRef(Box::new(inner))
                    } else {
                        Type::Ref(Box::new(inner))
                    },
                });
                if self.eat(&TokenKind::Comma) && !self.at(&TokenKind::RParen) {
                    loop {
                        let param_name = self.expect_ident("expected parameter name")?;
                        self.expect_simple(TokenKind::Colon, "expected `:` after parameter name")?;
                        let ty = self.parse_type()?;
                        params.push(Parameter {
                            name: param_name,
                            ty,
                        });
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
            } else if self.current_is_ident("self") {
                self.advance();
                params.push(Parameter {
                    name: "self".to_string(),
                    ty: Type::Struct(target.to_string()),
                });
                if self.eat(&TokenKind::Comma) && !self.at(&TokenKind::RParen) {
                    loop {
                        let param_name = self.expect_ident("expected parameter name")?;
                        self.expect_simple(TokenKind::Colon, "expected `:` after parameter name")?;
                        let ty = self.parse_type()?;
                        params.push(Parameter {
                            name: param_name,
                            ty,
                        });
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
            } else {
                loop {
                    let param_name = self.expect_ident("expected parameter name")?;
                    self.expect_simple(TokenKind::Colon, "expected `:` after parameter name")?;
                    let ty = self.parse_type()?;
                    params.push(Parameter {
                        name: param_name,
                        ty,
                    });
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
            }
        }
        self.expect_simple(TokenKind::RParen, "expected `)` after method parameters")?;
        let ret_type = if self.eat(&TokenKind::Arrow) {
            self.parse_type()?
        } else {
            Type::Unit
        };
        self.lifetime_scope = previous_lifetimes;
        let body = self.parse_block()?;
        Ok(Function {
            name,
            lifetime_params,
            params,
            ret_type,
            body,
            public: false,
            span,
        })
    }

    fn parse_lifetime_params(&mut self) -> Result<Vec<String>> {
        let mut lifetimes = Vec::new();
        if !self.eat(&TokenKind::Lt) {
            return Ok(lifetimes);
        }
        if self.at(&TokenKind::Gt) {
            return Err(MiniError::parse(
                "expected lifetime parameter",
                self.current().span,
            ));
        }
        loop {
            let span = self.current().span;
            let name = match self.current().kind.clone() {
                TokenKind::Lifetime(name) => {
                    self.advance();
                    name
                }
                _ => {
                    return Err(MiniError::parse(
                        "expected lifetime parameter like `'a`",
                        span,
                    ))
                }
            };
            if name == "static" {
                return Err(MiniError::parse(
                    "`'static` is reserved and cannot be declared as a lifetime parameter",
                    span,
                ));
            }
            if lifetimes.iter().any(|existing| existing == &name) {
                return Err(MiniError::parse(
                    format!("duplicate lifetime parameter `'{}`", name),
                    span,
                ));
            }
            lifetimes.push(name);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
            if self.at(&TokenKind::Gt) {
                break;
            }
        }
        self.expect_simple(TokenKind::Gt, "expected `>` after lifetime parameters")?;
        Ok(lifetimes)
    }

    fn enter_lifetime_scope(&mut self, lifetimes: &[String]) -> Vec<String> {
        let previous = self.lifetime_scope.clone();
        self.lifetime_scope = lifetimes.to_vec();
        previous
    }

    fn parse_optional_lifetime(&mut self) -> Result<Option<String>> {
        let span = self.current().span;
        let lifetime = match self.current().kind.clone() {
            TokenKind::Lifetime(name) => {
                self.advance();
                name
            }
            _ => return Ok(None),
        };
        if lifetime != "static" && !self.lifetime_scope.iter().any(|name| name == &lifetime) {
            return Err(MiniError::parse(
                format!("undeclared lifetime `'{}`", lifetime),
                span,
            ));
        }
        Ok(Some(lifetime))
    }

    fn parse_parameter_list(&mut self, self_type: &str) -> Result<Vec<Parameter>> {
        let mut params = Vec::new();
        if self.at(&TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            if self.eat(&TokenKind::Amp) {
                let _lifetime = self.parse_optional_lifetime()?;
                let mutable = self.eat(&TokenKind::Mut);
                if !self.current_is_ident("self") {
                    return Err(MiniError::parse(
                        "expected `self` after receiver `&`",
                        self.current().span,
                    ));
                }
                self.advance();
                let inner = Type::Struct(self_type.to_string());
                params.push(Parameter {
                    name: "self".to_string(),
                    ty: if mutable {
                        Type::MutRef(Box::new(inner))
                    } else {
                        Type::Ref(Box::new(inner))
                    },
                });
            } else if self.current_is_ident("self") {
                self.advance();
                params.push(Parameter {
                    name: "self".to_string(),
                    ty: Type::Struct(self_type.to_string()),
                });
            } else {
                let param_name = self.expect_ident("expected parameter name")?;
                self.expect_simple(TokenKind::Colon, "expected `:` after parameter name")?;
                let ty = self.parse_type()?;
                params.push(Parameter {
                    name: param_name,
                    ty,
                });
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
            if self.at(&TokenKind::RParen) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_type(&mut self) -> Result<Type> {
        if self.eat(&TokenKind::Amp) {
            let lifetime = self.parse_optional_lifetime()?;
            if self.eat(&TokenKind::Mut) {
                let inner = Box::new(self.parse_type()?);
                Ok(if let Some(lifetime) = lifetime {
                    Type::NamedMutRef(lifetime, inner)
                } else {
                    Type::MutRef(inner)
                })
            } else {
                let inner = Box::new(self.parse_type()?);
                Ok(if let Some(lifetime) = lifetime {
                    Type::NamedRef(lifetime, inner)
                } else {
                    Type::Ref(inner)
                })
            }
        } else if self.eat(&TokenKind::TypeI64) {
            Ok(Type::I64)
        } else if self.eat(&TokenKind::TypeF64) {
            Ok(Type::F64)
        } else if self.eat(&TokenKind::TypeBool) {
            Ok(Type::Bool)
        } else if self.eat(&TokenKind::TypeStr) {
            Ok(Type::Str)
        } else if self.eat(&TokenKind::TypeString) {
            Ok(Type::String)
        } else if self.eat(&TokenKind::LParen) {
            if self.eat(&TokenKind::RParen) {
                return Ok(Type::Unit);
            }
            let first = self.parse_type()?;
            if self.eat(&TokenKind::Comma) {
                let mut types = vec![first];
                if !self.at(&TokenKind::RParen) {
                    loop {
                        types.push(self.parse_type()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                        if self.at(&TokenKind::RParen) {
                            break;
                        }
                    }
                }
                self.expect_simple(TokenKind::RParen, "expected `)` after tuple type")?;
                Ok(Type::Tuple(types))
            } else {
                self.expect_simple(TokenKind::RParen, "expected `)` after grouped type")?;
                Ok(first)
            }
        } else if self.eat(&TokenKind::LBracket) {
            let elem = self.parse_type()?;
            self.expect_simple(TokenKind::Semi, "expected `;` in array type")?;
            let len = match self.current().kind.clone() {
                TokenKind::Int(value) if value >= 0 => {
                    self.advance();
                    value as usize
                }
                _ => {
                    return Err(MiniError::parse(
                        "expected array length",
                        self.current().span,
                    ))
                }
            };
            self.expect_simple(TokenKind::RBracket, "expected `]` after array type")?;
            Ok(Type::Array(Box::new(elem), len))
        } else if let TokenKind::Ident(name) = self.current().kind.clone() {
            self.advance();
            if name == "Vec" && self.eat(&TokenKind::Lt) {
                let elem = self.parse_type()?;
                self.expect_simple(TokenKind::Gt, "expected `>` after Vec element type")?;
                Ok(Type::Vec(Box::new(elem)))
            } else if name == "Option" && self.eat(&TokenKind::Lt) {
                let elem = self.parse_type()?;
                self.expect_simple(TokenKind::Gt, "expected `>` after Option element type")?;
                Ok(Type::Option(Box::new(elem)))
            } else if name == "Result" && self.eat(&TokenKind::Lt) {
                let ok = self.parse_type()?;
                self.expect_simple(TokenKind::Comma, "expected `,` in Result type")?;
                let err = self.parse_type()?;
                self.expect_simple(TokenKind::Gt, "expected `>` after Result error type")?;
                Ok(Type::Result(Box::new(ok), Box::new(err)))
            } else {
                Ok(Type::Struct(name))
            }
        } else {
            Err(MiniError::parse("expected type", self.current().span))
        }
    }

    fn parse_block(&mut self) -> Result<Block> {
        let span = self.expect_simple(TokenKind::LBrace, "expected `{`")?;
        let mut statements = Vec::new();
        let mut tail = None;
        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(MiniError::parse(
                    "expected `}` before end of file",
                    self.current().span,
                ));
            }
            if self.at(&TokenKind::Let) {
                statements.push(self.parse_let()?);
            } else if self.at(&TokenKind::Return) {
                statements.push(self.parse_return()?);
            } else if self.at(&TokenKind::Break) {
                statements.push(self.parse_break()?);
            } else if self.at(&TokenKind::Continue) {
                statements.push(self.parse_continue()?);
            } else if self.at(&TokenKind::While) {
                statements.push(self.parse_while()?);
            } else if self.at(&TokenKind::Loop) {
                statements.push(self.parse_loop()?);
            } else if self.at(&TokenKind::For) {
                statements.push(self.parse_for()?);
            } else {
                let expr = self.parse_expression()?;
                if self.eat(&TokenKind::Eq) {
                    let value = self.parse_expression()?;
                    let assign_span = expr.span();
                    self.expect_simple(TokenKind::Semi, "expected `;` after assignment")?;
                    statements.push(Statement::Assign {
                        target: expr,
                        value,
                        span: assign_span,
                    });
                } else if self.eat(&TokenKind::Semi) {
                    statements.push(Statement::Expr(expr));
                } else if matches!(expr, Expression::If { .. }) && !self.at(&TokenKind::RBrace) {
                    statements.push(Statement::Expr(expr));
                } else {
                    tail = Some(Box::new(expr));
                    break;
                }
            }
        }
        self.expect_simple(TokenKind::RBrace, "expected `}` after block")?;
        Ok(Block {
            statements,
            tail,
            span,
        })
    }

    fn parse_let(&mut self) -> Result<Statement> {
        let span = self.expect_simple(TokenKind::Let, "expected `let`")?;
        let mutable = self.eat(&TokenKind::Mut);
        let pattern = self.parse_let_pattern()?;
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect_simple(TokenKind::Eq, "expected `=` in let statement")?;
        let value = self.parse_expression()?;
        self.expect_simple(TokenKind::Semi, "expected `;` after let statement")?;
        Ok(Statement::Let {
            pattern,
            mutable,
            ty,
            value,
            span,
        })
    }

    fn parse_let_pattern(&mut self) -> Result<LetPattern> {
        if self.eat(&TokenKind::LParen) {
            if self.eat(&TokenKind::RParen) {
                return Ok(LetPattern::Unit);
            }
            let mut patterns = Vec::new();
            let mut saw_comma = false;
            loop {
                patterns.push(self.parse_let_pattern()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
                saw_comma = true;
                if self.at(&TokenKind::RParen) {
                    break;
                }
            }
            self.expect_simple(TokenKind::RParen, "expected `)` after let pattern")?;
            if patterns.len() == 1 && !saw_comma {
                Ok(patterns.remove(0))
            } else {
                Ok(LetPattern::Tuple(patterns))
            }
        } else {
            let name = self.expect_ident("expected variable name or pattern after `let`")?;
            if name == "_" {
                Ok(LetPattern::Wildcard)
            } else {
                Ok(LetPattern::Ident(name))
            }
        }
    }

    fn parse_return(&mut self) -> Result<Statement> {
        let span = self.expect_simple(TokenKind::Return, "expected `return`")?;
        let value = if self.at(&TokenKind::Semi) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_simple(TokenKind::Semi, "expected `;` after return statement")?;
        Ok(Statement::Return { value, span })
    }

    fn parse_break(&mut self) -> Result<Statement> {
        let span = self.expect_simple(TokenKind::Break, "expected `break`")?;
        self.expect_simple(TokenKind::Semi, "expected `;` after break statement")?;
        Ok(Statement::Break { span })
    }

    fn parse_continue(&mut self) -> Result<Statement> {
        let span = self.expect_simple(TokenKind::Continue, "expected `continue`")?;
        self.expect_simple(TokenKind::Semi, "expected `;` after continue statement")?;
        Ok(Statement::Continue { span })
    }

    fn parse_while(&mut self) -> Result<Statement> {
        let span = self.expect_simple(TokenKind::While, "expected `while`")?;
        let condition = self.parse_expression()?;
        let body = self.parse_block()?;
        Ok(Statement::While {
            condition,
            body,
            span,
        })
    }

    fn parse_loop(&mut self) -> Result<Statement> {
        let span = self.expect_simple(TokenKind::Loop, "expected `loop`")?;
        let body = self.parse_block()?;
        Ok(Statement::Loop { body, span })
    }

    fn parse_for(&mut self) -> Result<Statement> {
        let span = self.expect_simple(TokenKind::For, "expected `for`")?;
        let name = self.expect_ident("expected loop variable after `for`")?;
        self.expect_simple(TokenKind::In, "expected `in` after loop variable")?;
        let iterable = self.parse_expression()?;
        let body = self.parse_block()?;
        Ok(Statement::For {
            name,
            iterable,
            body,
            span,
        })
    }

    fn parse_expression(&mut self) -> Result<Expression> {
        let start = self.parse_precedence(0)?;
        if self.eat(&TokenKind::DotDot) {
            let end = self.parse_precedence(0)?;
            let span = start.span();
            Ok(Expression::Range {
                start: Box::new(start),
                end: Box::new(end),
                span,
            })
        } else {
            Ok(start)
        }
    }

    fn parse_precedence(&mut self, min_prec: u8) -> Result<Expression> {
        let mut left = self.parse_unary()?;
        while let Some((op, prec)) = self.current_binary_op() {
            if prec < min_prec {
                break;
            }
            self.advance();
            let right = self.parse_precedence(prec + 1)?;
            let span = left.span();
            left = Expression::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expression> {
        let span = self.current().span;
        if self.eat(&TokenKind::Minus) {
            Ok(Expression::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(self.parse_unary()?),
                span,
            })
        } else if self.eat(&TokenKind::Bang) {
            Ok(Expression::Unary {
                op: UnaryOp::Not,
                expr: Box::new(self.parse_unary()?),
                span,
            })
        } else if self.eat(&TokenKind::Star) {
            Ok(Expression::Deref {
                expr: Box::new(self.parse_unary()?),
                span,
            })
        } else if self.eat(&TokenKind::Amp) {
            let mutable = self.eat(&TokenKind::Mut);
            Ok(Expression::Ref {
                mutable,
                expr: Box::new(self.parse_unary()?),
                span,
            })
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expression> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.eat(&TokenKind::LBracket) {
                let index = self.parse_expression()?;
                let span = expr.span();
                self.expect_simple(TokenKind::RBracket, "expected `]` after index")?;
                expr = Expression::Index {
                    target: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
            } else if self.eat(&TokenKind::Dot) {
                let span = expr.span();
                let field = match self.current().kind.clone() {
                    TokenKind::Ident(name) => {
                        self.advance();
                        name
                    }
                    TokenKind::Int(value) if value >= 0 => {
                        self.advance();
                        value.to_string()
                    }
                    _ => {
                        return Err(MiniError::parse(
                            "expected field name after `.`",
                            self.current().span,
                        ))
                    }
                };
                if self.eat(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    if !self.at(&TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expression()?);
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect_simple(TokenKind::RParen, "expected `)` after method arguments")?;
                    expr = Expression::MethodCall {
                        receiver: Box::new(expr),
                        name: field,
                        args,
                        span,
                    };
                } else {
                    expr = Expression::Field {
                        target: Box::new(expr),
                        field,
                        span,
                    };
                }
            } else if self.eat(&TokenKind::Question) {
                let span = expr.span();
                expr = Expression::Try {
                    expr: Box::new(expr),
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expression> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Int(value) => {
                self.advance();
                Ok(Expression::Int(value, token.span))
            }
            TokenKind::Float(value) => {
                self.advance();
                Ok(Expression::Float(value, token.span))
            }
            TokenKind::String(value) => {
                self.advance();
                Ok(Expression::String(value, token.span))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expression::Bool(true, token.span))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expression::Bool(false, token.span))
            }
            TokenKind::Ident(name) => {
                self.advance();
                if name == "vec" && self.eat(&TokenKind::Bang) {
                    self.expect_simple(TokenKind::LBracket, "expected `[` after `vec!`")?;
                    let mut items = Vec::new();
                    if !self.at(&TokenKind::RBracket) {
                        loop {
                            items.push(self.parse_expression()?);
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                            if self.at(&TokenKind::RBracket) {
                                break;
                            }
                        }
                    }
                    self.expect_simple(TokenKind::RBracket, "expected `]` after vec literal")?;
                    Ok(Expression::Vec(items, token.span))
                } else if matches!(name.as_str(), "format" | "print" | "println")
                    && self.eat(&TokenKind::Bang)
                {
                    self.expect_simple(TokenKind::LParen, "expected `(` after macro name")?;
                    let mut args = Vec::new();
                    if !self.at(&TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expression()?);
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                            if self.at(&TokenKind::RParen) {
                                break;
                            }
                        }
                    }
                    self.expect_simple(TokenKind::RParen, "expected `)` after macro arguments")?;
                    let call_name = match name.as_str() {
                        "format" => "__format_macro",
                        "print" => "__print_macro",
                        "println" => "__println_macro",
                        _ => unreachable!(),
                    };
                    Ok(Expression::Call {
                        name: call_name.to_string(),
                        args,
                        span: token.span,
                    })
                } else if self.at(&TokenKind::ColonColon) {
                    let mut path = vec![name];
                    while self.eat(&TokenKind::ColonColon) {
                        path.push(self.expect_ident("expected path segment after `::`")?);
                    }
                    if path.len() == 2
                        && path[1]
                            .chars()
                            .next()
                            .is_some_and(|ch| ch.is_ascii_uppercase())
                    {
                        let value = if self.eat(&TokenKind::LParen) {
                            if self.eat(&TokenKind::RParen) {
                                None
                            } else {
                                let expr = self.parse_expression()?;
                                self.expect_simple(
                                    TokenKind::RParen,
                                    "expected `)` after enum value",
                                )?;
                                Some(Box::new(expr))
                            }
                        } else {
                            None
                        };
                        Ok(Expression::EnumLiteral {
                            enum_name: path[0].clone(),
                            variant: path[1].clone(),
                            value,
                            span: token.span,
                        })
                    } else if self.eat(&TokenKind::LParen) {
                        let mut args = Vec::new();
                        if !self.at(&TokenKind::RParen) {
                            loop {
                                args.push(self.parse_expression()?);
                                if !self.eat(&TokenKind::Comma) {
                                    break;
                                }
                            }
                        }
                        self.expect_simple(TokenKind::RParen, "expected `)` after arguments")?;
                        Ok(Expression::Call {
                            name: path.join("::"),
                            args,
                            span: token.span,
                        })
                    } else if path.len() == 2 {
                        Ok(Expression::EnumLiteral {
                            enum_name: path[0].clone(),
                            variant: path[1].clone(),
                            value: None,
                            span: token.span,
                        })
                    } else {
                        Err(MiniError::parse(
                            "expected function call after path",
                            self.current().span,
                        ))
                    }
                } else if self.eat(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    if !self.at(&TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expression()?);
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect_simple(TokenKind::RParen, "expected `)` after arguments")?;
                    Ok(Expression::Call {
                        name,
                        args,
                        span: token.span,
                    })
                } else if self.looks_like_struct_literal_body() && self.eat(&TokenKind::LBrace) {
                    let mut fields = Vec::new();
                    if !self.at(&TokenKind::RBrace) {
                        loop {
                            let field = self.expect_ident("expected struct literal field")?;
                            self.expect_simple(TokenKind::Colon, "expected `:` after field name")?;
                            let value = self.parse_expression()?;
                            fields.push((field, value));
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                            if self.at(&TokenKind::RBrace) {
                                break;
                            }
                        }
                    }
                    self.expect_simple(TokenKind::RBrace, "expected `}` after struct literal")?;
                    Ok(Expression::StructLiteral {
                        name,
                        fields,
                        span: token.span,
                    })
                } else {
                    Ok(Expression::Var(name, token.span))
                }
            }
            TokenKind::LParen => {
                self.advance();
                if self.eat(&TokenKind::RParen) {
                    Ok(Expression::Unit(token.span))
                } else {
                    let first = self.parse_expression()?;
                    if self.eat(&TokenKind::Comma) {
                        let mut items = vec![first];
                        if !self.at(&TokenKind::RParen) {
                            loop {
                                items.push(self.parse_expression()?);
                                if !self.eat(&TokenKind::Comma) {
                                    break;
                                }
                                if self.at(&TokenKind::RParen) {
                                    break;
                                }
                            }
                        }
                        self.expect_simple(TokenKind::RParen, "expected `)` after tuple")?;
                        Ok(Expression::Tuple(items, token.span))
                    } else {
                        self.expect_simple(TokenKind::RParen, "expected `)` after expression")?;
                        Ok(first)
                    }
                }
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                if !self.at(&TokenKind::RBracket) {
                    loop {
                        items.push(self.parse_expression()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                        if self.at(&TokenKind::RBracket) {
                            break;
                        }
                    }
                }
                self.expect_simple(TokenKind::RBracket, "expected `]` after array")?;
                Ok(Expression::Array(items, token.span))
            }
            TokenKind::LBrace => Ok(Expression::Block(self.parse_block()?)),
            TokenKind::If => self.parse_if(),
            TokenKind::Match => self.parse_match(),
            _ => Err(MiniError::parse("expected expression", token.span)),
        }
    }

    fn parse_if(&mut self) -> Result<Expression> {
        let span = self.expect_simple(TokenKind::If, "expected `if`")?;
        let condition = self.parse_expression()?;
        let then_block = self.parse_block()?;
        let else_block = if self.eat(&TokenKind::Else) {
            if self.at(&TokenKind::If) {
                let nested = self.parse_if()?;
                let nested_span = nested.span();
                Some(Block {
                    statements: Vec::new(),
                    tail: Some(Box::new(nested)),
                    span: nested_span,
                })
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(Expression::If {
            condition: Box::new(condition),
            then_block,
            else_block,
            span,
        })
    }

    fn parse_match(&mut self) -> Result<Expression> {
        let span = self.expect_simple(TokenKind::Match, "expected `match`")?;
        let value = self.parse_expression()?;
        self.expect_simple(TokenKind::LBrace, "expected `{` after match value")?;
        let mut arms = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            let arm_span = self.current().span;
            let pattern = self.parse_pattern()?;
            self.expect_simple(TokenKind::FatArrow, "expected `=>` after match pattern")?;
            let body = self.parse_expression()?;
            arms.push(MatchArm {
                pattern,
                body,
                span: arm_span,
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect_simple(TokenKind::RBrace, "expected `}` after match arms")?;
        Ok(Expression::Match {
            value: Box::new(value),
            arms,
            span,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Int(value) => {
                self.advance();
                Ok(Pattern::Int(value))
            }
            TokenKind::String(value) => {
                self.advance();
                Ok(Pattern::String(value))
            }
            TokenKind::True => {
                self.advance();
                Ok(Pattern::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Pattern::Bool(false))
            }
            TokenKind::LParen => {
                self.advance();
                if self.eat(&TokenKind::RParen) {
                    return Ok(Pattern::Unit);
                }
                let mut patterns = Vec::new();
                loop {
                    patterns.push(self.parse_pattern()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                    if self.at(&TokenKind::RParen) {
                        break;
                    }
                }
                self.expect_simple(TokenKind::RParen, "expected `)` after pattern")?;
                Ok(Pattern::Tuple(patterns))
            }
            TokenKind::Ident(name) => {
                self.advance();
                if name == "_" {
                    return Ok(Pattern::Wildcard);
                }
                if !self.eat_path_sep() {
                    return Ok(Pattern::Binding(name));
                }
                let variant = self.expect_ident("expected enum variant in pattern")?;
                let binding = if self.eat(&TokenKind::LParen) {
                    let binding = self.expect_ident("expected binding in enum pattern")?;
                    self.expect_simple(
                        TokenKind::RParen,
                        "expected `)` after enum pattern binding",
                    )?;
                    Some(binding)
                } else {
                    None
                };
                Ok(Pattern::EnumVariant {
                    enum_name: name,
                    variant,
                    binding,
                })
            }
            _ => Err(MiniError::parse("expected match pattern", token.span)),
        }
    }
    fn current_binary_op(&self) -> Option<(BinaryOp, u8)> {
        match self.current().kind {
            TokenKind::OrOr => Some((BinaryOp::Or, 1)),
            TokenKind::AndAnd => Some((BinaryOp::And, 2)),
            TokenKind::EqEq => Some((BinaryOp::Eq, 3)),
            TokenKind::BangEq => Some((BinaryOp::Ne, 3)),
            TokenKind::Lt => Some((BinaryOp::Lt, 4)),
            TokenKind::LtEq => Some((BinaryOp::Le, 4)),
            TokenKind::Gt => Some((BinaryOp::Gt, 4)),
            TokenKind::GtEq => Some((BinaryOp::Ge, 4)),
            TokenKind::Plus => Some((BinaryOp::Add, 5)),
            TokenKind::Minus => Some((BinaryOp::Sub, 5)),
            TokenKind::Star => Some((BinaryOp::Mul, 6)),
            TokenKind::Slash => Some((BinaryOp::Div, 6)),
            TokenKind::Percent => Some((BinaryOp::Rem, 6)),
            _ => None,
        }
    }

    fn expect_ident(&mut self, message: &str) -> Result<String> {
        match self.current().kind.clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(MiniError::parse(message, self.current().span)),
        }
    }

    fn expect_simple(&mut self, kind: TokenKind, message: &str) -> Result<Span> {
        if self.at(&kind) {
            let span = self.current().span;
            self.advance();
            Ok(span)
        } else {
            Err(MiniError::parse(message, self.current().span))
        }
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn current_is_ident(&self, expected: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Ident(name) if name == expected)
    }

    fn looks_like_struct_literal_body(&self) -> bool {
        if !self.at(&TokenKind::LBrace) {
            return false;
        }
        match self.tokens.get(self.pos + 1).map(|token| &token.kind) {
            Some(TokenKind::RBrace) => true,
            Some(TokenKind::Ident(_)) => matches!(
                self.tokens.get(self.pos + 2).map(|token| &token.kind),
                Some(TokenKind::Colon)
            ),
            _ => false,
        }
    }

    fn eat_path_sep(&mut self) -> bool {
        self.eat(&TokenKind::ColonColon) || self.eat(&TokenKind::Dot)
    }

    fn advance(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    #[test]
    fn parses_functions_and_refs() {
        let source = "fn inc(x: &mut i64) { *x = *x + 1; }";
        let tokens = Lexer::new(source).lex().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        assert_eq!(program.functions[0].name, "inc");
    }
}
