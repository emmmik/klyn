use crate::frame::Frame;
pub fn parse_frame(request: &Vec<String>) -> Option<(Frame, Vec<String>)> {
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
