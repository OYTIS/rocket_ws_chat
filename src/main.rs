#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate serde_derive;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate ws;
#[macro_use]
extern crate lazy_static;

use rocket::http::RawStr;
use rocket::http::Cookies;
use rocket::http::Cookie;
use rocket::response::NamedFile;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

#[get("/<is_hello>")]
fn index_bool(is_hello: bool) -> &'static str {
    if is_hello {
        "Hello, hello!"
    }
    else {
        "Not hello at all!"
    }
}

#[get("/<file..>", rank = 1)]
fn files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("/etc/").join(file)).ok()
}

#[get("/<name>", rank = 3)]
fn index(name: &RawStr) -> String {
    format!("Hello, {}!", name.as_str())
}

#[get("/cookies_set/<message>")]
fn cookies_set(message: &RawStr, mut cookies: Cookies) -> String {
    match cookies.get("message") {
        Some(_) => cookies.remove(Cookie::named("message")),
        None => (),
    };
    let cookie = Cookie::build("message", format!("{}", message))
        .path("/cookies_get")
        .finish();
    cookies.add(cookie);

    println!("Cookies?");
    for c in cookies.iter() {
        println!("Name: '{}', Value: '{}'", c.name(), c.value());
    };
    format!("Message set")
}

#[get("/cookies_get")]
fn cookies_get(cookies: Cookies) -> String {
    println!("Cookies!");
    for c in cookies.iter() {
        println!("Name: '{}', Value: '{}'", c.name(), c.value());
    };
    match cookies.get("message") {
        Some(cookie) => format!{"{}", cookie.value()},
        None => format!("{}", "Message not set"),
    }
}

/* Request fields:
 * type: login | ping | message
 * token (for ping, message)
 * uname (for login)
 * text (for message)
 */

/* Response fields:
 * type: login | ping | message
 * status: ok | failure
 * err (for errored requests)
 * token (for login)
 * messages: array of (uname, text) - for ping, message (successful)
 */
type ChatUser = String;
type ChatMessage = String;
type ChatToken = i32;

lazy_static! {
  static ref USERS : Mutex<std::collections::HashSet<ChatUser>> = Mutex::new(std::collections::HashSet::new());
  static ref USER_TOKENS : Mutex<std::collections::HashMap<ChatToken, ChatUser>> = Mutex::new(std::collections::HashMap::new());
  static ref MESSAGES : Mutex<std::vec::Vec<(ChatUser, ChatMessage)>> = Mutex::new(vec![]);
  static ref TOK_COUNTER : Mutex<i32> = Mutex::new(0);
}

fn conv_message(pair :&(ChatUser, ChatMessage)) -> serde_json::Value {
    let mut res : serde_json::Value = json!({});
    res[pair.clone().0] = json!(pair.clone().1);
    res
}

fn process_login(value: &serde_json::Value) -> serde_json::Value {
    let ref mut users = *USERS.lock().unwrap();
    let ref mut user_tokens = *USER_TOKENS.lock().unwrap();
    let ref messages = *MESSAGES.lock().unwrap();
    let ref uname = value["uname"];
    let mut res : serde_json::Value = json!({"type":"login"});

    if let &serde_json::Value::String(ref u) = uname {
        if users.contains(u) {
            res["status"] = json!("failure");
            res["err"] = json!("already_exists");
        } else {
            users.insert(u.clone());
            let ref mut tok = *TOK_COUNTER.lock().unwrap();
            user_tokens.insert(*tok, u.clone());
            res["status"] = json!("success");
            res["token"] = json!(tok);
            res["messages"] = messages.iter().map(conv_message).collect(); 
            *tok += 1;
        }
    } else {
        res["status"] = json!("failure");
        res["err"] = json!("format");
    }
    res
}

fn process_ping(value: &serde_json::Value) -> serde_json::Value {
    let mut res : serde_json::Value = json!({"type":"ping"});
    let ref user_tokens = *USER_TOKENS.lock().unwrap();
    let ref messages = *MESSAGES.lock().unwrap();
    let ref token = value["token"];

    if let &serde_json::Value::Number(ref t) = token {
        if !user_tokens.contains_key(&(t.as_i64().unwrap() as i32)) {
            res["status"] = json!("failure");
            res["err"] = json!("authentication_failed");
        } else {
            res["status"] = json!("success");
            res["messages"] = messages.iter().map(conv_message).collect();
        }
    } else {
        res["status"] = json!("failure");
        res["err"] = json!("format");
    }
    res
}

fn process_message(value: &serde_json::Value) -> serde_json::Value {
    let mut res : serde_json::Value = json!({"type":"message"});
    let ref users = *USERS.lock().unwrap();
    let ref user_tokens = *USER_TOKENS.lock().unwrap();
    let ref mut messages = *MESSAGES.lock().unwrap();
    let ref token = value["token"];

    if let &serde_json::Value::Number(ref t) = token {
        if let Some(user) = user_tokens.get(&(t.as_i64().unwrap() as i32)) {
            let ref message_v = value["message"];
            if let &serde_json::Value::String(ref message) = message_v {
                messages.push((user.clone(), message.clone()));
                res["status"] = json!("success");
                res["messages"] = messages.iter().map(conv_message).collect();
            } else {
                res["status"] = json!("failure");
                res["err"] = json!("format");
            }
        } else {
            res["status"] = json!("failure");
            res["err"] = json!("authentication_failed");
        }
    } else {
        res["status"] = json!("failure");
        res["err"] = json!("format");
    }
    res
}

fn main() {
    std::thread::spawn(move|| {
        ws::listen("127.0.0.1:3012", move |out| {
            move |msg| {
                match msg {
                    ws::Message::Text(text) => {
                        let v = match serde_json::from_str(text.as_str()) {
                            Ok(value) => value,
                            _ => serde_json::Value::Null,
                        };
                        let resp_value = if let serde_json::Value::String(ref req_type) = v["type"] {
                            match req_type.as_str() {
                                "login" => process_login(&v),
                                "ping" => process_ping(&v),
                                "message" => process_message(&v),
                                _ => json!({"status":"failure","err":"unsupported"})
                            }
                        } else {
                            json!({"status":"failure","err":"format"})
                        };
                        out.send(resp_value.to_string())
                                            }
                    ws::Message::Binary(_) => return Err(ws::Error{kind : ws::ErrorKind::Internal, details: std::borrow::Cow::Borrowed("Binary messages are not supported")})
                }
            }
        })
    });
    rocket::ignite().mount("/", routes![index, cookies_get, cookies_set]).launch();
}

