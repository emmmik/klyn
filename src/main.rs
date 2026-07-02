use std::{
    io::prelude::*,
    net::{TcpListener, TcpStream},
    str,
};

#[derive(Debug, PartialEq)]
enum Frame {
    SimpleString(String),
    SimpleError(String),
    BulkString(String),
    Array(Vec<Frame>),
}

impl Frame {
    fn get_value(&self) -> Option<&String> {
        match self {
            Self::SimpleString(s) => Some(s),
            Self::SimpleError(s) => Some(s),
            Self::BulkString(s) => Some(s),
            _ => None,
        }
    }

    fn get_array(&self) -> Option<&Vec<Frame>> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        handle_connection(stream);
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 512];
    let buffer_size = stream.read(&mut buffer).unwrap();
    let request_string = str::from_utf8(&buffer[..buffer_size]).unwrap();
    let request_vector: Vec<String> = request_string
        .split("\r\n")
        .map(|s| s.to_string())
        .collect();

    let frame = parse_frame(&request_vector).unwrap().0;
    if frame == Frame::Array(vec![Frame::BulkString(String::from("PING"))]) {
        stream.write_all(
            encode_frame(&Frame::SimpleString(String::from("PONG")))
                .unwrap()
                .as_bytes(),
        );
        stream.flush();
    }
    // println!("{:#?}", parse_frame(&request_vector));
    // println!(
    //     "{:#?}",
    //     encode_frame(&parse_frame(&request_vector).unwrap().0)
    // );
}

fn parse_frame(request: &Vec<String>) -> Option<(Frame, Vec<String>)> {
    let first_command = String::from(&request[0]);
    let first_command_rest = &first_command[1..];

    match first_command.as_bytes()[0] {
        b'+' => Some((
            Frame::SimpleString(String::from(first_command_rest)),
            request[1..].to_vec(),
        )),
        b'-' => Some((
            Frame::SimpleError(String::from(first_command_rest)),
            request[1..].to_vec(),
        )),
        b'$' => Some((
            Frame::BulkString(String::from(&request[1])),
            request[2..].to_vec(),
        )),
        b'*' => {
            let mut array_elements: Vec<Frame> = Vec::new();

            let mut array_size = 0;
            for i in 1..first_command.len() {
                array_size = array_size * 10 + first_command.as_bytes()[i] - b'0';
            }
            let mut rem = request[1..].to_vec();
            for i in 0..array_size {
                let element = parse_frame(&rem).unwrap();
                array_elements.push(element.0);
                rem = element.1;
            }

            Some((Frame::Array(array_elements), rem))
        }
        // (b'-', data) => Some(Frame::SimpleError),
        // Some(b':') => Some(Frame::Integer),
        _ => None,
    }
}

fn encode_frame(request: &Frame) -> Option<String> {
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
        Frame::BulkString(s) => {
            converted_string += "$";
            converted_string += &s.len().to_string();
            converted_string += "\r\n";
            converted_string += s;
            converted_string += "\r\n";

            Some(converted_string)
        }
        Frame::Array(arr) => {
            converted_string += "*";
            converted_string += &arr.len().to_string();
            converted_string += "\r\n";
            for element in arr {
                converted_string += &encode_frame(element).unwrap();
            }
            Some(converted_string)
        }

        _ => None,
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     #[test]
//     fn test_simple_string() {
//         let input = b"+OK\r\n";
//         let frame: Option<Frame> = Some(parse_frame(str::from_utf8(input).unwrap().1);
//         assert_eq!(frame, Some(Frame::SimpleString(String::from("OK\r\n"))));
//     }
// }
