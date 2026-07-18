use crate::frame::Frame;
pub fn encode_frame(request: &Frame) -> Option<String> {
    let mut converted_string = String::new();
    match request {
        Frame::SimpleString(s) => {
            converted_string += "+";
            converted_string += s;
            converted_string += "\r\n";

            Some(converted_string)
        }
        Frame::SimpleError(s) => {
            converted_string += "-";
            converted_string += s;
            converted_string += "\r\n";

            Some(converted_string)
        }
        Frame::Integer(num) => {
            converted_string = ":".to_string();
            converted_string += &num.to_string();
            converted_string += "\r\n";

            Some(converted_string)
        }
        Frame::BulkString(Some(s)) => {
            converted_string += "$";
            converted_string += &s.len().to_string();
            converted_string += "\r\n";
            converted_string += s;
            converted_string += "\r\n";

            Some(converted_string)
        }
        Frame::BulkString(None) => Some("$-1\r\n".to_string()),
        Frame::Array(arr) => {
            converted_string += "*";
            converted_string += &arr.len().to_string();
            converted_string += "\r\n";
            for element in arr {
                converted_string += &encode_frame(element).unwrap();
            }
            Some(converted_string)
        }
    }
}
