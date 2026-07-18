#[derive(Debug, PartialEq)]
pub enum Frame {
    SimpleString(String),
    SimpleError(String),
    BulkString(Option<String>),
    Array(Vec<Frame>),
    Integer(i32),
}

impl Frame {
    pub fn get_value(&self) -> Option<&String> {
        match self {
            Self::SimpleString(s) => Some(s),
            Self::SimpleError(s) => Some(s),
            Self::BulkString(Some(s)) => Some(s),
            _ => None,
        }
    }

    pub fn get_array(&self) -> Option<&Vec<Frame>> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }
}
