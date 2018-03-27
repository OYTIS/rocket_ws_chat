#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate rocket;
#[macro_use]
extern crate serde_derive;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate ws;
extern crate lazy_static;

extern crate jsonwebtoken;

use rocket::http::RawStr;
use rocket::http::Cookies;
use rocket::http::Cookie;
use rocket::response::NamedFile;
use std::env;
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
    if cookies.get("message").is_some() {
        cookies.remove(Cookie::named("message"));
    }
    let cookie = Cookie::build("message", format!("{}", message))
        .path("/cookies_get")
        .finish();
    cookies.add(cookie);

    println!("Cookies?");
    for c in cookies.iter() {
        println!("Name: '{}', Value: '{}'", c.name(), c.value());
    };
    "Message set".to_string()
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
struct WsChat {
  users : Mutex<std::collections::HashSet<ChatUser>>,
  messages : Mutex<std::vec::Vec<(ChatUser, ChatMessage)>>,
  secret : String,
}

#[derive(Serialize, Deserialize)]
struct UserClaims {
    sub : String,
}

impl WsChat {
    pub fn new(secret : String) -> WsChat {
        let users = Mutex::new(std::collections::HashSet::new());
        let messages = Mutex::new(vec![]);

        WsChat {
            users,
            messages,
            secret
        }
    }
    fn conv_message(pair :&(ChatUser, ChatMessage)) -> serde_json::Value {
        let mut res : serde_json::Value = json!({});
        res["uname"] = json!(pair.clone().0);
        res["message"] = json!(pair.clone().1);
        res
    }

    fn process_login(&self, value: &serde_json::Value) -> serde_json::Value {
        let uname = &value["uname"];
        let mut res : serde_json::Value = json!({"type":"login"});

        if let serde_json::Value::String(ref u) = *uname {
            let mut users = match self.users.lock() {
                Ok(u) => u,
                Err(_) => return json!({"status":"failure", "err":"system"}),
            };
            if users.contains(u) {
                res["status"] = json!("failure");
                res["err"] = json!("already_exists");
            } else {
                let messages = match self.messages.lock() {
                    Ok(m) => m,
                    Err(_) => return json!({"status":"failure", "err":"system"}),
                };

                users.insert(u.clone());
                res["status"] = json!("success");

                let claims = UserClaims {
                    sub : u.clone(),
                };
                let token = match jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims, self.secret.as_ref()) {
                    Ok(t) => t,
                    Err(_) => return json!({"status":"failure", "err":"authentication failed"})
                };
                res["token"] = json!(token);
                res["messages"] = messages.iter().map(WsChat::conv_message).collect(); 
            }
        } else {
            res["status"] = json!("failure");
            res["err"] = json!("format");
        }
        res
    }

    fn process_ping(&self, value: &serde_json::Value) -> serde_json::Value {
        let mut res : serde_json::Value = json!({"type":"ping"});
        let token = &value["token"];

        if let serde_json::Value::String(ref s) = *token {
            if jsonwebtoken::decode::<UserClaims>(s, self.secret.as_ref(), &jsonwebtoken::Validation::default()).is_err() {
                return json!({"status":"failure","err":"authentication_failed"});
            }
            res["status"] = json!("success");
            let messages = match self.messages.lock() {
                Ok(m) => m,
                Err(_) => return json!({"status":"failure", "err":"system"}),
            };
            res["messages"] = messages.iter().map(WsChat::conv_message).collect();
        } else {
            res["status"] = json!("failure");
            res["err"] = json!("format");
        }
        res
    }

    fn process_message(&self, value: &serde_json::Value) -> serde_json::Value {
        let mut res : serde_json::Value = json!({"type":"message"});
        let token = &value["token"];

        if let serde_json::Value::String(ref s) = *token {

            if let Ok(claim) = jsonwebtoken::decode::<UserClaims>(s, self.secret.as_ref(), &jsonwebtoken::Validation::default()) {
                let message_v = &value["message"];
                if let serde_json::Value::String(ref message) = *message_v {
                    let mut messages = match self.messages.lock() {
                        Ok(m) => m,
                        Err(_) => return json!({"status":"failure", "err":"system"}),
                    };
                    messages.push((claim.claims.sub.clone(), message.clone()));
                    res["status"] = json!("success");
                    res["messages"] = messages.iter().map(WsChat::conv_message).collect();
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
    
    pub fn dispatch(&self, value: &serde_json::Value) -> serde_json::Value {
        if let serde_json::Value::String(ref req_type) = value["type"] {
            match req_type.as_str() {
                "login" => self.process_login(value),
                "ping" => self.process_ping(value),
                "message" => self.process_message(value),
                _ => json!({"status":"failure","err":"unsupported"})
            }
        } else {
            json!({"status":"failure","err":"format"})
        }
    }
}





fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 1 {
        std::process::exit(1);
    }

    let secret = args[1].clone();
    let ws_chat = Arc::new(WsChat::new(secret));
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
                        let resp_value = ws_chat.dispatch(&v);

                        out.send(resp_value.to_string())
                    }
                    ws::Message::Binary(_) => Err(ws::Error{kind : ws::ErrorKind::Internal, details: std::borrow::Cow::Borrowed("Binary messages are not supported")})
                }
            }
        })
    });
    rocket::ignite().mount("/", routes![files, cookies_get, cookies_set]).launch();
}

