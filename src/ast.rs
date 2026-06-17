use crate::error::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub modules: Vec<ModuleImport>,
    pub uses: Vec<UseImport>,
    pub traits: Vec<TraitDef>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub impls: Vec<ImplBlock>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleImport {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseImport {
    pub path: Vec<String>,
    pub glob: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplBlock {
    pub target: String,
    pub trait_name: Option<String>,
    pub methods: Vec<Function>,
    pub public: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitDef {
    pub name: String,
    pub methods: Vec<TraitMethod>,
    pub public: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<Parameter>,
    pub ret_type: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub public: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: String,
    pub payload: Option<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
    pub public: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<Parameter>,
    pub ret_type: Type,
    pub body: Block,
    pub public: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    I64,
    F64,
    Bool,
    Str,
    String,
    Unit,
    Range,
    Tuple(Vec<Type>),
    Array(Box<Type>, usize),
    Vec(Box<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Struct(String),
    Enum(String),
    Ref(Box<Type>),
    MutRef(Box<Type>),
}

impl Type {
    pub fn is_copy(&self) -> bool {
        match self {
            Type::I64 | Type::F64 | Type::Bool | Type::Unit | Type::Ref(_) | Type::MutRef(_) => {
                true
            }
            Type::Range => true,
            Type::Tuple(items) => items.iter().all(Type::is_copy),
            Type::Array(item, _) => item.is_copy(),
            Type::Option(item) => item.is_copy(),
            Type::Result(ok, err) => ok.is_copy() && err.is_copy(),
            Type::Str | Type::String | Type::Vec(_) | Type::Struct(_) | Type::Enum(_) => false,
        }
    }

    pub fn contains_ref(&self) -> bool {
        match self {
            Type::Ref(_) | Type::MutRef(_) => true,
            Type::Tuple(items) => items.iter().any(Type::contains_ref),
            Type::Array(item, _) | Type::Vec(item) => item.contains_ref(),
            Type::Option(item) => item.contains_ref(),
            Type::Result(ok, err) => ok.contains_ref() || err.contains_ref(),
            Type::I64
            | Type::F64
            | Type::Bool
            | Type::Range
            | Type::Str
            | Type::String
            | Type::Unit
            | Type::Struct(_)
            | Type::Enum(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub tail: Option<Box<Expression>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Let {
        name: String,
        mutable: bool,
        ty: Option<Type>,
        value: Expression,
        span: Span,
    },
    Assign {
        target: Expression,
        value: Expression,
        span: Span,
    },
    Expr(Expression),
    Return {
        value: Option<Expression>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    While {
        condition: Expression,
        body: Block,
        span: Span,
    },
    Loop {
        body: Block,
        span: Span,
    },
    For {
        name: String,
        iterable: Expression,
        body: Block,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Int(i64, Span),
    Float(u64, Span),
    Bool(bool, Span),
    String(String, Span),
    Tuple(Vec<Expression>, Span),
    Array(Vec<Expression>, Span),
    Vec(Vec<Expression>, Span),
    Range {
        start: Box<Expression>,
        end: Box<Expression>,
        span: Span,
    },
    Unit(Span),
    Var(String, Span),
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expression>,
        span: Span,
    },
    Call {
        name: String,
        args: Vec<Expression>,
        span: Span,
    },
    MethodCall {
        receiver: Box<Expression>,
        name: String,
        args: Vec<Expression>,
        span: Span,
    },
    StructLiteral {
        name: String,
        fields: Vec<(String, Expression)>,
        span: Span,
    },
    EnumLiteral {
        enum_name: String,
        variant: String,
        value: Option<Box<Expression>>,
        span: Span,
    },
    Index {
        target: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },
    Field {
        target: Box<Expression>,
        field: String,
        span: Span,
    },
    Block(Block),
    If {
        condition: Box<Expression>,
        then_block: Block,
        else_block: Option<Block>,
        span: Span,
    },
    Match {
        value: Box<Expression>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Ref {
        mutable: bool,
        expr: Box<Expression>,
        span: Span,
    },
    Deref {
        expr: Box<Expression>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Wildcard,
    EnumVariant {
        enum_name: String,
        variant: String,
        binding: Option<String>,
    },
}

impl Expression {
    pub fn span(&self) -> Span {
        match self {
            Expression::Int(_, span)
            | Expression::Float(_, span)
            | Expression::Bool(_, span)
            | Expression::String(_, span)
            | Expression::Tuple(_, span)
            | Expression::Array(_, span)
            | Expression::Vec(_, span)
            | Expression::Range { span, .. }
            | Expression::Unit(span)
            | Expression::Var(_, span)
            | Expression::Binary { span, .. }
            | Expression::Unary { span, .. }
            | Expression::Call { span, .. }
            | Expression::MethodCall { span, .. }
            | Expression::StructLiteral { span, .. }
            | Expression::EnumLiteral { span, .. }
            | Expression::Index { span, .. }
            | Expression::Field { span, .. }
            | Expression::If { span, .. }
            | Expression::Match { span, .. }
            | Expression::Ref { span, .. }
            | Expression::Deref { span, .. } => *span,
            Expression::Block(block) => block.span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}
