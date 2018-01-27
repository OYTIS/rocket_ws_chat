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
use std::sync::Arc;

#[get("/<file..>", rank = 1)]
fn files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).ok()
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
struct WsChat {
  user_tokens : std::collections::HashMap<ChatToken, ChatUser>,
  messages : std::vec::Vec<(ChatUser, ChatMessage)>,
  tok_counter : i32,
}

impl WsChat {
    pub fn new() -> WsChat {
        let user_tokens = std::collections::HashMap::new();
        let messages = vec![];
        let tok_counter = 0;

        WsChat {
            user_tokens : user_tokens,
            messages : messages,
            tok_counter : tok_counter
        }
    }
    fn conv_message(pair :&(ChatUser, ChatMessage)) -> serde_json::Value {
        let mut res : serde_json::Value = json!({});
        res["uname"] = json!(pair.clone().0);
        res["message"] = json!(pair.clone().1);
        res
    }

    fn process_login(&mut self, value: &serde_json::Value) -> serde_json::Value {
        let ref uname = value["uname"];
        let mut res : serde_json::Value = json!({"type":"login"});

        if let &serde_json::Value::String(ref u) = uname {
            //if self.user_tokens.contains_key(u) {
            //    res["status"] = json!("failure");
            //    res["err"] = json!("already_exists");
            //} else {
                self.user_tokens.insert(self.tok_counter, u.clone());
                res["status"] = json!("success");
                res["token"] = json!(self.tok_counter);
                res["messages"] = self.messages.iter().map(WsChat::conv_message).collect(); 
                self.tok_counter += 1;
            //}
        } else {
            res["status"] = json!("failure");
            res["err"] = json!("format");
        }
        res
    }

    fn process_ping(&mut self, value: &serde_json::Value) -> serde_json::Value {
        let mut res : serde_json::Value = json!({"type":"ping"});
        let ref token = value["token"];

        if let &serde_json::Value::Number(ref t) = token {
            if !self.user_tokens.contains_key(&(t.as_i64().unwrap() as i32)) {
                res["status"] = json!("failure");
                res["err"] = json!("authentication_failed");
            } else {
                res["status"] = json!("success");
                res["messages"] = self.messages.iter().map(WsChat::conv_message).collect();
            }
        } else {
            res["status"] = json!("failure");
            res["err"] = json!("format");
        }
        res
    }

    fn process_message(&mut self, value: &serde_json::Value) -> serde_json::Value {
        let mut res : serde_json::Value = json!({"type":"message"});
        let ref token = value["token"];

        if let &serde_json::Value::Number(ref t) = token {
            if let Some(user) = self.user_tokens.get(&(t.as_i64().unwrap() as i32)) {
                let ref message_v = value["message"];
                if let &serde_json::Value::String(ref message) = message_v {
                    self.messages.push((user.clone(), message.clone()));
                    res["status"] = json!("success");
                    res["messages"] = self.messages.iter().map(WsChat::conv_message).collect();
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
    
    pub fn dispatch(&mut self, value: &serde_json::Value) -> serde_json::Value {
        if let serde_json::Value::String(ref req_type) = value["type"] {
            match req_type.as_str() {
                "login" => self.process_login(&value),
                "ping" => self.process_ping(&value),
                "message" => self.process_message(&value),
                _ => json!({"status":"failure","err":"unsupported"})
            }
        } else {
            json!({"status":"failure","err":"format"})
        }
    }
}





fn main() {
    let ws_chat = Arc::new(Mutex::new(WsChat::new()));
    std::thread::spawn(move|| {
        ws::listen("127.0.0.1:3012", move |out| {
            let ws_chat = ws_chat.clone();
            move |msg| {
                match msg {
                    ws::Message::Text(text) => {
                        let v = match serde_json::from_str(text.as_str()) {
                            Ok(value) => value,
                            _ => serde_json::Value::Null,
                        };
                        let mut ws_chat_unlocked = ws_chat.lock().unwrap();
                        let resp_value = ws_chat_unlocked.dispatch(&v);
                        out.send(resp_value.to_string())
                    }
                    ws::Message::Binary(_) => return Err(ws::Error{kind : ws::ErrorKind::Internal, details: std::borrow::Cow::Borrowed("Binary messages are not supported")})
                }
            }
        })
    });
    rocket::ignite().mount("/", routes![files, cookies_get, cookies_set]).launch();
}

