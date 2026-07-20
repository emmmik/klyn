use crate::encoder;
use crate::frame::Frame;

use std::{
    collections::HashMap,
    fs::File,
    io::prelude::*,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub fn set(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
    key: &String,
    value: &String,
) {
    let mut hm = counter_db.lock().unwrap();
    let mut file = counter_aof.lock().unwrap();

    hm.insert(key.clone(), (value.clone(), None));

    let command_frame = Frame::Array(vec![
        Frame::BulkString(Some("SET".to_string())),
        Frame::BulkString(Some(key.clone())),
        Frame::BulkString(Some(value.clone())),
    ]);
    let command_string = encoder::encode_frame(&command_frame).unwrap();
    file.write_all(command_string.as_bytes()).unwrap();
}

pub fn del(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
    key: &String,
) -> Option<Frame> {
    let mut hm = counter_db.lock().unwrap();
    let mut file = counter_aof.lock().unwrap();

    match hm.get(key) {
        Some(_value) => {
            hm.remove(key);
            let command_frame = Frame::Array(vec![
                Frame::BulkString(Some("DEL".to_string())),
                Frame::BulkString(Some(key.clone())),
            ]);
            let command_string = encoder::encode_frame(&command_frame).unwrap();
            file.write_all(command_string.as_bytes()).unwrap();
            Some(Frame::Integer(1))
        }
        _ => Some(Frame::Integer(0)),
    }
}

pub fn get(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
    key: &String,
) -> Option<(String, Option<Instant>)> {
    let mut hm = counter_db.lock().unwrap();
    let mut file = counter_aof.lock().unwrap();

    match hm.get(key) {
        Some(value) => {
            if value.1 == None || value.1.unwrap() >= Instant::now() {
                // Some(Frame::BulkString(Some(value.0.to_string())))
                Some(value.clone())
            } else {
                hm.remove(key);
                let command_frame = Frame::Array(vec![
                    Frame::BulkString(Some("DEL".to_string())),
                    Frame::BulkString(Some(key.clone())),
                ]);
                let command_string = encoder::encode_frame(&command_frame).unwrap();
                file.write_all(command_string.as_bytes()).unwrap();
                None
            }
        }
        _ => None,
    }
}

pub fn exists(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
    key: &String,
) -> Option<Frame> {
    match get(counter_db, counter_aof, key) {
        Some(_s) => Some(Frame::Integer(1)),
        _ => Some(Frame::Integer(0)),
    }
}

pub fn ttl(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
    key: &String,
) -> Option<Frame> {
    match get(counter_db, counter_aof, key) {
        Some(value) if value.1 == None => Some(Frame::Integer(-1)),
        Some(value) if value.1.is_some() => Some(Frame::Integer(
            (value.1.unwrap() - Instant::now()).as_secs() as i32,
        )),
        _ => Some(Frame::Integer(-2)),
    }
}

pub fn expire(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    key: &String,
    new_duration: u64,
) -> Option<Frame> {
    let mut hm = counter_db.lock().unwrap();

    match hm.get_mut(key) {
        Some(value) => {
            value.1 = Some(Instant::now() + Duration::from_secs(new_duration));
            Some(Frame::Integer(1))
        }
        _ => Some(Frame::Integer(0)),
    }
}

pub fn persist(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
    key: &String,
) -> Option<Frame> {
    let mut hm = counter_db.lock().unwrap();
    let mut file = counter_aof.lock().unwrap();

    match hm.get_mut(key) {
        Some(value) => {
            if value.1 == None {
                Some(Frame::Integer(0))
            } else if value.1.unwrap() >= Instant::now() {
                value.1 = None;
                Some(Frame::Integer(1))
            } else {
                hm.remove(key);
                let command_frame = Frame::Array(vec![
                    Frame::BulkString(Some("DEL".to_string())),
                    Frame::BulkString(Some(key.clone())),
                ]);
                let command_string = encoder::encode_frame(&command_frame).unwrap();
                file.write_all(command_string.as_bytes()).unwrap();
                Some(Frame::Integer(0))
            }
        }
        _ => Some(Frame::Integer(0)),
    }
}

pub fn incr(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    key: &String,
) -> Option<Frame> {
    let mut hm = counter_db.lock().unwrap();

    match hm.get_mut(key) {
        Some(value) => {
            let parsed_key = value.0.parse::<i32>();
            match parsed_key {
                Ok(num) => {
                    value.0 = (num + 1).to_string();
                    Some(Frame::Integer(num + 1))
                }
                Err(_err) => Some(Frame::SimpleError(
                    "ERR value is not an integer or out of range".to_string(),
                )),
            }
        }
        _ => {
            hm.insert(key.clone(), ("1".to_string(), None));
            Some(Frame::Integer(1))
        }
    }
}

pub fn decr(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    key: &String,
) -> Option<Frame> {
    let mut hm = counter_db.lock().unwrap();

    match hm.get_mut(key) {
        Some(value) => {
            let parsed_key = value.0.parse::<i32>();
            match parsed_key {
                Ok(num) => {
                    value.0 = (num - 1).to_string();
                    Some(Frame::Integer(num - 1))
                }
                Err(_err) => Some(Frame::SimpleError(
                    "ERR value is not an integer or out of range".to_string(),
                )),
            }
        }
        _ => {
            hm.insert(key.clone(), ("-1".to_string(), None));
            Some(Frame::Integer(-1))
        }
    }
}

pub fn keys(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
) -> Option<Frame> {
    let mut hm = counter_db.lock().unwrap();
    let mut file = counter_aof.lock().unwrap();

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

pub fn flushdb(
    counter_db: &Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    counter_aof: &Arc<Mutex<File>>,
) -> Option<Frame> {
    let mut hm = counter_db.lock().unwrap();
    let mut file = counter_aof.lock().unwrap();

    hm.clear();

    let command_frame = Frame::Array(vec![Frame::BulkString(Some("FLUSHDB".to_string()))]);
    let command_string = encoder::encode_frame(&command_frame).unwrap();
    file.write_all(command_string.as_bytes()).unwrap();

    Some(Frame::SimpleString("OK".to_string()))
}
