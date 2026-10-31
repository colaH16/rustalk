use serde::Serialize;
use std::{collections::HashMap, hint::black_box, time::Instant};
#[derive(Serialize)]
struct Message<'a> {
    id: i32,
    content: &'a str,
    sender_pod: &'a str,
    sender_nick: &'a str,
    sender_color: &'a str,
    time: &'a str,
}
fn fields(encoded: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for (k, v) in url::form_urlencoded::parse(encoded.as_bytes()) {
        result.entry(k.into_owned()).or_insert(v.into_owned());
    }
    result
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let kind = &args[1];
    let n: usize = args[2].parse().unwrap();
    let m = Message {
        id: 42,
        content: "hello Rustalk",
        sender_pod: "bench-pod",
        sender_nick: "bench-user",
        sender_color: "#abcdef",
        time: "12:34:56",
    };
    let input = "nick=bench-user&color=%23abcdef&msg=hello+Rustalk%21";
    let op = || {
        if kind == "json" {
            black_box(serde_json::to_vec(black_box(&m)).unwrap());
        } else {
            black_box(fields(black_box(input)));
        }
    };
    for _ in 0..10000 {
        op();
    }
    let start = Instant::now();
    for _ in 0..n {
        op();
    }
    let elapsed = start.elapsed();
    println!(
        "{}",
        serde_json::json!({"case":kind,"iterations":n,"ns_per_op":elapsed.as_nanos() as f64/n as f64,"sample_json":serde_json::to_string(&m).unwrap(),"sample_fields":fields(input)})
    );
}
