use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::prelude::*,
    net::{TcpListener, TcpStream},
    str,
    sync::{Arc, Mutex},
    thread,
};

#[derive(Debug, PartialEq)]
enum Frame {
    SimpleString(String),
    SimpleError(String),
    BulkString(Option<String>),
    Array(Vec<Frame>),
    Integer(i32),
}

impl Frame {
    fn get_value(&self) -> Option<&String> {
        match self {
            Self::SimpleString(s) => Some(s),
            Self::SimpleError(s) => Some(s),
            Self::BulkString(Some(s)) => Some(s),
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

    let db: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let aof = Arc::new(Mutex::new(
        OpenOptions::new()
            .append(true)
            .read(true)
            .open("klyn.aof")
            .unwrap(),
    ));
    let file_string = std::fs::read_to_string("klyn.aof").unwrap();
    let file_vector: Vec<String> = file_string.split("\r\n").map(|s| s.to_string()).collect();
    let mut rem: Vec<String> = file_vector;
    rem.pop();
    while let Some((frame, remaining)) = parse_frame(&rem) {
        let counter = Arc::clone(&db);
        let mut hm = counter.lock().unwrap();

        let elements = frame.get_array().unwrap();
        match &elements[0] {
            Frame::BulkString(Some(s)) if s == "SET" => {
                hm.insert(
                    elements[1].get_value().unwrap().to_string(),
                    elements[2].get_value().unwrap().to_string(),
                );
            }
            Frame::BulkString(Some(s)) if s == "DEL" => {
                hm.remove(&elements[1].get_value().unwrap().to_string());
            }
            _ => (),
        };
        rem = remaining;
    }

    let mut handles = vec![];
    for stream in listener.incoming() {
        let counter_db = Arc::clone(&db);
        let counter_aof = Arc::clone(&aof);

        let handle = thread::spawn(move || {
            let mut hm = counter_db.lock().unwrap();
            let mut file = counter_aof.lock().unwrap();
            let stream = stream.unwrap();

            handle_connection(stream, &mut hm, &mut file);
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
}

fn handle_connection(mut stream: TcpStream, hm: &mut HashMap<String, String>, file: &mut File) {
    let mut buffer = [0; 512];
    let buffer_size = stream.read(&mut buffer).unwrap();
    let request_string = str::from_utf8(&buffer[..buffer_size]).unwrap();
    let request_vector: Vec<String> = request_string
        .split("\r\n")
        .map(|s| s.to_string())
        .collect();

    let frame = parse_frame(&request_vector).unwrap().0;

    let response: Option<Frame> = match frame {
        Frame::Array(elements) => match &elements[0] {
            Frame::BulkString(Some(s)) if s == "PING" => {
                Some(Frame::SimpleString("PONG".to_string()))
            }
            Frame::BulkString(Some(s)) if s == "SET" => {
                hm.insert(
                    elements[1].get_value().unwrap().to_string(),
                    elements[2].get_value().unwrap().to_string(),
                );

                let command_frame = Frame::Array(vec![
                    Frame::BulkString(Some("SET".to_string())),
                    Frame::BulkString(Some(elements[1].get_value().unwrap().to_string())),
                    Frame::BulkString(Some(elements[2].get_value().unwrap().to_string())),
                ]);
                let command_string = encode_frame(&command_frame).unwrap();
                file.write_all(command_string.as_bytes()).unwrap();

                Some(Frame::SimpleString("OK".to_string()))
            }
            Frame::BulkString(Some(s)) if s == "GET" => {
                match hm.get(&elements[1].get_value().unwrap().to_string()) {
                    Some(value) => Some(Frame::BulkString(Some(value.to_string()))),
                    _ => Some(Frame::BulkString(None)),
                }
            }
            Frame::BulkString(Some(s)) if s == "DEL" => {
                match &hm.get(&elements[1].get_value().unwrap().to_string()) {
                    Some(_value) => {
                        hm.remove(&elements[1].get_value().unwrap().to_string());
                        let command_frame = Frame::Array(vec![
                            Frame::BulkString(Some("DEL".to_string())),
                            Frame::BulkString(Some(elements[1].get_value().unwrap().to_string())),
                        ]);
                        let command_string = encode_frame(&command_frame).unwrap();
                        file.write_all(command_string.as_bytes()).unwrap();
                        Some(Frame::Integer(1))
                    }
                    _ => Some(Frame::Integer(0)),
                }
            }
            Frame::BulkString(Some(s)) if s == "EXISTS" => {
                match &hm.get(&elements[1].get_value().unwrap().to_string()) {
                    Some(_value) => Some(Frame::Integer(1)),
                    _ => Some(Frame::Integer(0)),
                }
            }
            Frame::BulkString(Some(s)) if s == "KEYS" => {
                let mut keys: Vec<Frame> = Vec::new();
                let values: Vec<_> = hm.clone().into_keys().collect();

                for key in values {
                    keys.push(Frame::BulkString(Some(key)));
                }
                Some(Frame::Array(keys))
            }
            _ => Some(Frame::SimpleError("ERR Unknown command".to_string())),
        },
        _ => None,
    };

    stream
        .write_all(encode_frame(&response.unwrap()).unwrap().as_bytes())
        .unwrap();
}

fn parse_frame(request: &Vec<String>) -> Option<(Frame, Vec<String>)> {
    if request.is_empty() {
        return None;
    }
    let first_command = &request[0];
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
        b':' => Some((
            Frame::Integer(first_command_rest.parse::<i32>().unwrap()),
            request[1..].to_vec(),
        )),
        b'$' => Some((
            Frame::BulkString(Some(String::from(&request[1]))),
            request[2..].to_vec(),
        )),
        b'*' => {
            let mut array_elements: Vec<Frame> = Vec::new();

            let mut array_size = 0;
            for i in 1..first_command.len() {
                array_size = array_size * 10 + first_command.as_bytes()[i] - b'0';
            }
            let mut rem = request[1..].to_vec();
            for _i in 0..array_size {
                let element = parse_frame(&rem).unwrap();
                array_elements.push(element.0);
                rem = element.1;
            }

            Some((Frame::Array(array_elements), rem))
        }
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
