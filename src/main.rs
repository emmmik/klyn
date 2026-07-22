mod db;
mod encoder;
mod frame;
mod parser;
#[cfg(test)]
mod tests;

use frame::Frame;

use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::prelude::*,
    net::{TcpListener, TcpStream},
    str,
    sync::{Arc, Mutex},
    thread,
    time::Instant,
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
            Frame::BulkString(Some(s)) if s == "FLUSHDB" => {
                hm.clear();
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
    let response: Option<Frame> = match frame {
        Frame::Array(elements) => match &elements[0] {
            Frame::BulkString(Some(s)) if s == "PING" => {
                Some(Frame::SimpleString("PONG".to_string()))
            }
            Frame::BulkString(Some(s)) if s == "SET" => {
                db::set(
                    counter_db,
                    counter_aof,
                    &elements[1].get_value().unwrap().to_string(),
                    &elements[2].get_value().unwrap().to_string(),
                );
                Some(Frame::SimpleString("OK".to_string()))
            }
            Frame::BulkString(Some(s)) if s == "EXPIRE" => db::expire(
                counter_db,
                &elements[1].get_value().unwrap().to_string(),
                elements[2].get_value().unwrap().parse::<u64>().unwrap(),
            ),
            Frame::BulkString(Some(s)) if s == "TTL" => db::ttl(
                counter_db,
                counter_aof,
                &elements[1].get_value().unwrap().to_string(),
            ),
            Frame::BulkString(Some(s)) if s == "PERSIST" => db::persist(
                counter_db,
                counter_aof,
                &elements[1].get_value().unwrap().to_string(),
            ),
            Frame::BulkString(Some(s)) if s == "GET" => {
                let value = db::get(
                    counter_db,
                    counter_aof,
                    &elements[1].get_value().unwrap().to_string(),
                );
                match value {
                    Some(content) => Some(Frame::BulkString(Some(content.0.to_string()))),
                    _ => Some(Frame::BulkString(None)),
                }
            }
            Frame::BulkString(Some(s)) if s == "DEL" => db::del(
                counter_db,
                counter_aof,
                &elements[1].get_value().unwrap().to_string(),
            ),
            Frame::BulkString(Some(s)) if s == "EXISTS" => db::exists(
                counter_db,
                counter_aof,
                &elements[1].get_value().unwrap().to_string(),
            ),
            Frame::BulkString(Some(s)) if s == "INCR" => db::incr(
                counter_db,
                counter_aof,
                &elements[1].get_value().unwrap().to_string(),
            ),
            Frame::BulkString(Some(s)) if s == "DECR" => db::decr(
                counter_db,
                counter_aof,
                &elements[1].get_value().unwrap().to_string(),
            ),
            Frame::BulkString(Some(s)) if s == "RENAME" => db::rename(
                counter_db,
                counter_aof,
                &elements[1].get_value().unwrap().to_string(),
                &elements[2].get_value().unwrap().to_string(),
            ),
            Frame::BulkString(Some(s)) if s == "KEYS" => db::keys(counter_db, counter_aof),
            Frame::BulkString(Some(s)) if s == "FLUSHDB" => db::flushdb(counter_db, counter_aof),
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
