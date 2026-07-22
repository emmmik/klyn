use crate::{db, frame::Frame};

use std::{
    collections::HashMap,
    fs::OpenOptions,
    sync::{Arc, Mutex},
    time::Instant,
};

fn setup() -> (
    Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>,
    Arc<Mutex<std::fs::File>>,
) {
    let db = Arc::new(Mutex::new(HashMap::new()));
    let file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("/tmp/klyn_test.aof")
            .unwrap(),
    ));
    (db, file)
}

#[test]
fn test_set_and_get() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"name".to_string(), &"emmanuel".to_string());
    let result = db::get(&db, &aof, &"name".to_string());
    assert_eq!(result.unwrap().0, "emmanuel");
}

#[test]
fn test_get_missing_key() {
    let (db, aof) = setup();
    let result = db::get(&db, &aof, &"nonexistent".to_string());
    assert_eq!(result, None);
}

#[test]
fn test_del_existing_key() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"x".to_string(), &"1".to_string());
    let result = db::del(&db, &aof, &"x".to_string());
    assert_eq!(result, Some(Frame::Integer(1)));
    assert_eq!(db::get(&db, &aof, &"x".to_string()), None);
}

#[test]
fn test_del_missing_key() {
    let (db, aof) = setup();
    let result = db::del(&db, &aof, &"x".to_string());
    assert_eq!(result, Some(Frame::Integer(0)));
}

#[test]
fn test_exists_after_set() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"k".to_string(), &"v".to_string());
    let result = db::exists(&db, &aof, &"k".to_string());
    assert_eq!(result, Some(Frame::Integer(1)));
}

#[test]
fn test_exists_missing() {
    let (db, aof) = setup();
    let result = db::exists(&db, &aof, &"k".to_string());
    assert_eq!(result, Some(Frame::Integer(0)));
}

#[test]
fn test_incr_new_key() {
    let (db, aof) = setup();
    let _ = db::set(&db, &aof, &"_p".to_string(), &"_".to_string());
    let result = db::incr(&db, &aof, &"counter".to_string());
    assert_eq!(result, Some(Frame::Integer(1)));
}

#[test]
fn test_incr_existing_key() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"counter".to_string(), &"5".to_string());
    let result = db::incr(&db, &aof, &"counter".to_string());
    assert_eq!(result, Some(Frame::Integer(6)));
}

#[test]
fn test_decr_new_key() {
    let (db, aof) = setup();
    let _ = db::set(&db, &aof, &"_p".to_string(), &"_".to_string());
    let result = db::decr(&db, &aof, &"c".to_string());
    assert_eq!(result, Some(Frame::Integer(-1)));
}

#[test]
fn test_incr_non_integer_value() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"key".to_string(), &"hello".to_string());
    let result = db::incr(&db, &aof, &"key".to_string());
    assert_eq!(
        result,
        Some(Frame::SimpleError(
            "ERR value is not an integer or out of range".to_string()
        ))
    );
}

#[test]
fn test_expire_and_ttl() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"tmp".to_string(), &"value".to_string());

    let expire_result = db::expire(&db, &"tmp".to_string(), 60);
    assert_eq!(expire_result, Some(Frame::Integer(1)));

    let ttl_result = db::ttl(&db, &aof, &"tmp".to_string());
    if let Some(Frame::Integer(secs)) = ttl_result {
        assert!(
            secs <= 60 && secs > 55,
            "TTL should be ~60 seconds, got {}",
            secs
        );
    } else {
        panic!("Expected Integer frame, got {:?}", ttl_result);
    }
}

#[test]
fn test_expire_missing_key() {
    let (db, _aof) = setup();
    let result = db::expire(&db, &"nonexistent".to_string(), 10);
    assert_eq!(result, Some(Frame::Integer(0)));
}

#[test]
fn test_ttl_no_expiry() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"persistent".to_string(), &"val".to_string());
    let result = db::ttl(&db, &aof, &"persistent".to_string());
    assert_eq!(result, Some(Frame::Integer(-1)));
}

#[test]
fn test_ttl_missing_key() {
    let (db, aof) = setup();
    let result = db::ttl(&db, &aof, &"ghost".to_string());
    assert_eq!(result, Some(Frame::Integer(-2)));
}

#[test]
fn test_persist() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"tmp".to_string(), &"val".to_string());
    db::expire(&db, &"tmp".to_string(), 60);

    let persist_result = db::persist(&db, &aof, &"tmp".to_string());
    assert_eq!(persist_result, Some(Frame::Integer(1)));

    let ttl_result = db::ttl(&db, &aof, &"tmp".to_string());
    assert_eq!(ttl_result, Some(Frame::Integer(-1)));
}

#[test]
fn test_flushdb() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"a".to_string(), &"1".to_string());
    db::set(&db, &aof, &"b".to_string(), &"2".to_string());

    let flush_result = db::flushdb(&db, &aof);
    assert_eq!(flush_result, Some(Frame::SimpleString("OK".to_string())));

    assert_eq!(db::get(&db, &aof, &"a".to_string()), None);
    assert_eq!(db::get(&db, &aof, &"b".to_string()), None);
}

#[test]
fn test_keys() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"x".to_string(), &"1".to_string());
    db::set(&db, &aof, &"y".to_string(), &"2".to_string());

    let keys_result = db::keys(&db, &aof);
    if let Some(Frame::Array(frames)) = keys_result {
        let mut names: Vec<String> = frames
            .iter()
            .map(|f| match f {
                Frame::BulkString(Some(s)) => s.clone(),
                _ => panic!("expected BulkString in keys"),
            })
            .collect();
        names.sort();
        assert_eq!(names, vec!["x".to_string(), "y".to_string()]);
    } else {
        panic!("Expected Array frame, got {:?}", keys_result);
    }
}

#[test]
fn test_rename_success() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"old_key".to_string(), &"value".to_string());

    let result = db::rename(&db, &aof, &"old_key".to_string(), &"new_key".to_string());
    assert_eq!(result, Some(Frame::SimpleString("OK".to_string())));

    assert_eq!(db::get(&db, &aof, &"old_key".to_string()), None);
    let value = db::get(&db, &aof, &"new_key".to_string());
    assert_eq!(value.unwrap().0, "value");
}

#[test]
fn test_rename_same_key() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"key".to_string(), &"value".to_string());

    let result = db::rename(&db, &aof, &"key".to_string(), &"key".to_string());
    assert_eq!(
        result,
        Some(Frame::SimpleError(
            "ERR Source and destination keys are the same".to_string()
        ))
    );
}

#[test]
fn test_rename_missing_source() {
    let (db, aof) = setup();

    let result = db::rename(
        &db,
        &aof,
        &"nonexistent".to_string(),
        &"new_key".to_string(),
    );
    assert_eq!(
        result,
        Some(Frame::SimpleError("ERR No such key".to_string()))
    );
}

#[test]
fn test_rename_target_exists() {
    let (db, aof) = setup();
    db::set(&db, &aof, &"old_key".to_string(), &"value".to_string());
    db::set(&db, &aof, &"new_key".to_string(), &"other".to_string());

    let result = db::rename(&db, &aof, &"old_key".to_string(), &"new_key".to_string());
    assert_eq!(
        result,
        Some(Frame::SimpleError(
            "ERR Targer key name is busy".to_string()
        ))
    );
}
