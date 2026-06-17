use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Float(u64),
    Bool(bool),
    String(String),
    Tuple(Vec<Value>),
    Array(Vec<Value>),
    Vec(Vec<Value>),
    Struct {
        name: String,
        fields: Vec<(String, Value)>,
    },
    Enum {
        enum_name: String,
        variant: String,
        value: Option<Box<Value>>,
    },
    Unit,
    Range(i64, i64),
    Ref(RefValue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefValue {
    pub frame: usize,
    pub name: String,
    pub mutable: bool,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{}", v),
            Value::Float(v) => {
                let value = f64::from_bits(*v);
                write!(f, "{}", value)
            }
            Value::Bool(v) => write!(f, "{}", v),
            Value::String(v) => write!(f, "{}", v),
            Value::Tuple(items) => {
                let text = items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({})", text)
            }
            Value::Array(items) => {
                let text = items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{}]", text)
            }
            Value::Vec(items) => {
                let text = items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "vec![{}]", text)
            }
            Value::Struct { name, fields } => {
                let text = fields
                    .iter()
                    .map(|(field, value)| format!("{}: {}", field, value))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} {{ {} }}", name, text)
            }
            Value::Enum {
                enum_name,
                variant,
                value,
            } => {
                if let Some(value) = value {
                    write!(f, "{}::{}({})", enum_name, variant, value)
                } else {
                    write!(f, "{}::{}", enum_name, variant)
                }
            }
            Value::Unit => write!(f, "()"),
            Value::Range(start, end) => write!(f, "{}..{}", start, end),
            Value::Ref(r) => write!(f, "&{}", r.name),
        }
    }
}
