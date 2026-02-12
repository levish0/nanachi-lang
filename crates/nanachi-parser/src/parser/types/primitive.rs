use nanachi_ast::types::PrimitiveType;

pub fn parse_primitive(name: &str) -> Option<PrimitiveType> {
    match name {
        "i8" => Some(PrimitiveType::I8),
        "i16" => Some(PrimitiveType::I16),
        "i32" => Some(PrimitiveType::I32),
        "i64" => Some(PrimitiveType::I64),
        "i128" => Some(PrimitiveType::I128),
        "u8" => Some(PrimitiveType::U8),
        "u16" => Some(PrimitiveType::U16),
        "u32" => Some(PrimitiveType::U32),
        "u64" => Some(PrimitiveType::U64),
        "u128" => Some(PrimitiveType::U128),
        "f32" => Some(PrimitiveType::F32),
        "f64" => Some(PrimitiveType::F64),
        "bool" => Some(PrimitiveType::Bool),
        "char" => Some(PrimitiveType::Char),
        "usize" => Some(PrimitiveType::Usize),
        "isize" => Some(PrimitiveType::Isize),
        _ => None,
    }
}
