mod encoder;
mod frame;
mod parser;

use frame::Frame;

use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::prelude::*,
    net::{TcpListener, TcpStream},
    str,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

fn main() {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();

    let db: Arc<Mutex<HashMap<String, (String, Option<Instant>)>>> =
        Arc::new(Mutex::new(HashMap::new()));
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
    while let Some((frame, remaining)) = parser::parse_frame(&rem) {
        let counter = Arc::clone(&db);
        let mut hm = counter.lock().unwrap();

        let elements = frame.get_array().unwrap();
        match &elements[0] {
            Frame::BulkString(Some(s)) if s == "SET" => {
                hm.insert(
                    elements[1].get_value().unwrap().to_string(),
                    (elements[2].get_value().unwrap().to_string(), None),
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
            // let mut hm = counter_db.lock().unwrap();
            // let mut file = counter_aof.lock().unwrap();
            let stream = stream.unwrap();

            handle_connection(stream, &counter_db, &counter_aof);
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
}

fn handle_connection(
    mut stream: TcpStream,
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
) {
    let mut buffer = [0; 512];
    let buffer_size = stream.read(&mut buffer).unwrap();
    let request_string = str::from_utf8(&buffer[..buffer_size]).unwrap();
    let request_vector: Vec<String> = request_string
        .split("\r\n")
        .map(|s| s.to_string())
        .collect();

    let frame = parser::parse_frame(&request_vector).unwrap().0;
    let mut hm = counter_db.lock().unwrap();
    let mut file = counter_aof.lock().unwrap();
    let response: Option<Frame> = match frame {
        Frame::Array(elements) => match &elements[0] {
            Frame::BulkString(Some(s)) if s == "PING" => {
                Some(Frame::SimpleString("PONG".to_string()))
            }
            Frame::BulkString(Some(s)) if s == "SET" => {
                hm.insert(
                    elements[1].get_value().unwrap().to_string(),
                    (elements[2].get_value().unwrap().to_string(), None),
                );

                let command_frame = Frame::Array(vec![
                    Frame::BulkString(Some("SET".to_string())),
                    Frame::BulkString(Some(elements[1].get_value().unwrap().to_string())),
                    Frame::BulkString(Some(elements[2].get_value().unwrap().to_string())),
                ]);
                let command_string = encoder::encode_frame(&command_frame).unwrap();
                file.write_all(command_string.as_bytes()).unwrap();

                Some(Frame::SimpleString("OK".to_string()))
            }
            Frame::BulkString(Some(s)) if s == "EXPIRE" => {
                match hm.get_mut(&elements[1].get_value().unwrap().to_string()) {
                    Some(value) => {
                        value.1 = Some(
                            Instant::now()
                                + Duration::from_secs(
                                    elements[2].get_value().unwrap().parse::<u64>().unwrap(),
                                ),
                        );
                        Some(Frame::Integer(1))
                    }
                    _ => Some(Frame::Integer(0)),
                }
            }
            Frame::BulkString(Some(s)) if s == "TTL" => {
                match hm.get(&elements[1].get_value().unwrap().to_string()) {
                    Some(value) => {
                        if value.1 == None {
                            Some(Frame::Integer(-1))
                        } else if value.1.unwrap() >= Instant::now() {
                            Some(Frame::Integer(
                                (value.1.unwrap() - Instant::now()).as_secs() as i32,
                            ))
                        } else {
                            hm.remove(&elements[1].get_value().unwrap().to_string());
                            let command_frame = Frame::Array(vec![
                                Frame::BulkString(Some("DEL".to_string())),
                                Frame::BulkString(Some(
                                    elements[1].get_value().unwrap().to_string(),
                                )),
                            ]);
                            let command_string = encoder::encode_frame(&command_frame).unwrap();
                            file.write_all(command_string.as_bytes()).unwrap();
                            Some(Frame::Integer(-2))
                        }
                    }
                    _ => Some(Frame::Integer(-2)),
                }
            }
            Frame::BulkString(Some(s)) if s == "PERSIST" => {
                match hm.get_mut(&elements[1].get_value().unwrap().to_string()) {
                    Some(value) => {
                        if value.1 == None {
                            Some(Frame::Integer(0))
                        } else if value.1.unwrap() >= Instant::now() {
                            value.1 = None;
                            Some(Frame::Integer(1))
                        } else {
                            hm.remove(&elements[1].get_value().unwrap().to_string());
                            let command_frame = Frame::Array(vec![
                                Frame::BulkString(Some("DEL".to_string())),
                                Frame::BulkString(Some(
                                    elements[1].get_value().unwrap().to_string(),
                                )),
                            ]);
                            let command_string = encoder::encode_frame(&command_frame).unwrap();
                            file.write_all(command_string.as_bytes()).unwrap();
                            Some(Frame::Integer(0))
                        }
                    }
                    _ => Some(Frame::Integer(0)),
                }
            }
            Frame::BulkString(Some(s)) if s == "GET" => {
                match hm.get(&elements[1].get_value().unwrap().to_string()) {
                    Some(value) => {
                        if value.1 == None || value.1.unwrap() >= Instant::now() {
                            Some(Frame::BulkString(Some(value.0.to_string())))
                        } else {
                            hm.remove(&elements[1].get_value().unwrap().to_string());
                            let command_frame = Frame::Array(vec![
                                Frame::BulkString(Some("DEL".to_string())),
                                Frame::BulkString(Some(
                                    elements[1].get_value().unwrap().to_string(),
                                )),
                            ]);
                            let command_string = encoder::encode_frame(&command_frame).unwrap();
                            file.write_all(command_string.as_bytes()).unwrap();
                            Some(Frame::BulkString(None))
                        }
                    }
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
                        let command_string = encoder::encode_frame(&command_frame).unwrap();
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
                    if let Some(value) = hm.get(&key) {
                        if value.1 == None || value.1.unwrap() >= Instant::now() {
                            keys.push(Frame::BulkString(Some(key)));
                        } else {
                            hm.remove(&key);
                            let command_frame = Frame::Array(vec![
                                Frame::BulkString(Some("DEL".to_string())),
                                Frame::BulkString(Some(key)),
                            ]);
                            let command_string = encoder::encode_frame(&command_frame).unwrap();
                            file.write_all(command_string.as_bytes()).unwrap();
                        }
                    }
                }
                Some(Frame::Array(keys))
            }
            _ => Some(Frame::SimpleError("ERR Unknown command".to_string())),
        },
        _ => None,
    };

    stream
        .write_all(
            encoder::encode_frame(&response.unwrap())
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
}
