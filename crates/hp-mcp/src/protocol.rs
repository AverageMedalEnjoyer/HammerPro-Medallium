use std::io::{self, BufRead, Write};

use serde_json::{Map, Value, json};

pub const SUPPORTED_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
pub const LATEST_VERSION: &str = SUPPORTED_VERSIONS[0];

pub mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

pub fn negotiate_version(requested: Option<&str>) -> &'static str {
    requested.and_then(|r| SUPPORTED_VERSIONS.iter().copied().find(|v| *v == r)).unwrap_or(LATEST_VERSION)
}

fn has_structured_content(version: &str) -> bool {
    version >= "2025-06-18"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Message(String),
    TooLarge,
    NotUtf8,
    Eof,
}

pub fn read_message<R: BufRead>(reader: &mut R, max: usize) -> io::Result<Frame> {
    let mut buf: Vec<u8> = Vec::new();
    let mut too_large = false;
    loop {
        let available = match reader.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if available.is_empty() {
            if too_large {
                return Ok(Frame::TooLarge);
            }
            return Ok(match finish_line(buf) {
                Some(frame) => frame,
                None => Frame::Eof,
            });
        }
        let newline = available.iter().position(|&b| b == b'\n');
        let chunk = &available[..newline.unwrap_or(available.len())];
        if !too_large {
            if buf.len() + chunk.len() > max {
                too_large = true;
                buf = Vec::new();
            } else {
                buf.extend_from_slice(chunk);
            }
        }
        let used = chunk.len() + usize::from(newline.is_some());
        reader.consume(used);
        if newline.is_some() {
            if too_large {
                return Ok(Frame::TooLarge);
            }
            if let Some(frame) = finish_line(std::mem::take(&mut buf)) {
                return Ok(frame);
            }
        }
    }
}

fn finish_line(mut buf: Vec<u8>) -> Option<Frame> {
    while buf.last().is_some_and(|b| b.is_ascii_whitespace()) {
        buf.pop();
    }
    if buf.iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    Some(match String::from_utf8(buf) {
        Ok(s) => Frame::Message(s),
        Err(_) => Frame::NotUtf8,
    })
}

pub fn write_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let mut text = serde_json::to_string(message).map_err(io::Error::other)?;
    text.push('\n');
    writer.write_all(text.as_bytes())?;
    writer.flush()
}

pub fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

pub fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn request(id: Value, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

pub fn notification(method: &str) -> Value {
    json!({ "jsonrpc": "2.0", "method": method })
}

#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Request { id: Value, method: String, params: Value },
    Notification { method: String },
    Response,
}

pub fn classify(msg: &Value) -> Result<Incoming, Value> {
    let Some(obj) = msg.as_object() else {
        return Err(error_response(Value::Null, codes::INVALID_REQUEST, "Invalid request: not an object"));
    };
    let id = obj.get("id").cloned();
    let valid_id = match &id {
        None => true,
        Some(Value::String(_)) => true,
        Some(Value::Number(n)) => n.is_i64() || n.is_u64(),
        Some(_) => false,
    };
    let answer_id = if valid_id { id.clone().unwrap_or(Value::Null) } else { Value::Null };
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(error_response(answer_id, codes::INVALID_REQUEST, "Invalid request: jsonrpc must be \"2.0\""));
    }
    if !valid_id {
        return Err(error_response(Value::Null, codes::INVALID_REQUEST, "Invalid request: bad id"));
    }
    match (obj.get("method"), id) {
        (Some(Value::String(method)), Some(id)) => Ok(Incoming::Request {
            id,
            method: method.clone(),
            params: obj.get("params").cloned().unwrap_or(Value::Null),
        }),
        (Some(Value::String(method)), None) => Ok(Incoming::Notification { method: method.clone() }),
        (Some(_), _) => {
            Err(error_response(answer_id, codes::INVALID_REQUEST, "Invalid request: method must be a string"))
        }
        (None, Some(_)) if obj.contains_key("result") || obj.contains_key("error") => Ok(Incoming::Response),
        (None, _) => Err(error_response(answer_id, codes::INVALID_REQUEST, "Invalid request: no method")),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(String),
    Image { data: String, mime_type: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub content: Vec<Content>,
    pub structured: Option<Value>,
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self { content: vec![Content::Text(text.into())], structured: None, is_error: false }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self { content: vec![Content::Text(text.into())], structured: None, is_error: true }
    }

    pub fn json(value: Value) -> Self {
        let text = serde_json::to_string_pretty(&value).unwrap_or_default();
        let structured = value.is_object().then_some(value);
        Self { content: vec![Content::Text(text)], structured, is_error: false }
    }

    pub fn png(png: &[u8], caption: impl Into<String>) -> Self {
        Self {
            content: vec![
                Content::Image { data: crate::b64::encode(png), mime_type: "image/png".to_owned() },
                Content::Text(caption.into()),
            ],
            structured: None,
            is_error: false,
        }
    }

    pub fn first_text(&self) -> &str {
        self.content
            .iter()
            .find_map(|c| match c {
                Content::Text(t) => Some(t.as_str()),
                Content::Image { .. } => None,
            })
            .unwrap_or("")
    }

    pub fn to_value(&self, version: &str) -> Value {
        let content: Vec<Value> = self
            .content
            .iter()
            .map(|c| match c {
                Content::Text(text) => json!({ "type": "text", "text": text }),
                Content::Image { data, mime_type } => json!({ "type": "image", "data": data, "mimeType": mime_type }),
            })
            .collect();
        let mut out = Map::new();
        out.insert("content".into(), Value::Array(content));
        if let Some(s) = &self.structured
            && has_structured_content(version)
        {
            out.insert("structuredContent".into(), s.clone());
        }
        out.insert("isError".into(), Value::Bool(self.is_error));
        Value::Object(out)
    }

    pub fn from_value(value: &Value) -> Option<Self> {
        let obj = value.as_object()?;
        let mut content = Vec::new();
        for item in obj.get("content")?.as_array()? {
            match item.get("type").and_then(Value::as_str) {
                Some("text") => content.push(Content::Text(item.get("text")?.as_str()?.to_owned())),
                Some("image") => content.push(Content::Image {
                    data: item.get("data")?.as_str()?.to_owned(),
                    mime_type: item.get("mimeType")?.as_str()?.to_owned(),
                }),
                _ => {}
            }
        }
        Some(Self {
            content,
            structured: obj.get("structuredContent").cloned(),
            is_error: obj.get("isError").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ServerInfo {
    pub name: String,
    pub title: String,
    pub version: String,
    pub instructions: Option<String>,
}

impl ServerInfo {
    pub fn hammer_pro() -> Self {
        Self {
            name: crate::SERVER_NAME.to_owned(),
            title: crate::PRODUCT_NAME.to_owned(),
            version: crate::VERSION.to_owned(),
            instructions: Some(crate::INSTRUCTIONS.to_owned()),
        }
    }
}

pub trait ToolHost {
    fn list_tools(&mut self) -> Vec<Value>;

    fn call_tool(&mut self, name: &str, arguments: Value, version: &'static str) -> Option<ToolResult>;
}

#[derive(Debug)]
pub struct Session {
    info: ServerInfo,
    version: Option<&'static str>,
    initialized: bool,
}

impl Session {
    pub fn new(info: ServerInfo) -> Self {
        Self { info, version: None, initialized: false }
    }

    pub fn version(&self) -> &'static str {
        self.version.unwrap_or(LATEST_VERSION)
    }

    pub fn is_initialized(&self) -> bool {
        self.version.is_some()
    }

    pub fn client_ready(&self) -> bool {
        self.initialized
    }

    pub fn handle_frame(&mut self, frame: &Frame, host: &mut dyn ToolHost) -> Option<Value> {
        match frame {
            Frame::Message(text) => self.handle_text(text, host),
            Frame::TooLarge => Some(error_response(
                Value::Null,
                codes::INVALID_REQUEST,
                format!("Message too large (limit {} bytes)", crate::MAX_MESSAGE_BYTES),
            )),
            Frame::NotUtf8 => Some(error_response(Value::Null, codes::PARSE_ERROR, "Parse error: not UTF-8")),
            Frame::Eof => None,
        }
    }

    pub fn handle_text(&mut self, text: &str, host: &mut dyn ToolHost) -> Option<Value> {
        match serde_json::from_str::<Value>(text) {
            Ok(msg) => self.handle(msg, host),
            Err(e) => Some(error_response(Value::Null, codes::PARSE_ERROR, format!("Parse error: {e}"))),
        }
    }

    pub fn handle(&mut self, msg: Value, host: &mut dyn ToolHost) -> Option<Value> {
        match msg {
            Value::Array(items) if items.is_empty() => {
                Some(error_response(Value::Null, codes::INVALID_REQUEST, "Invalid request: empty batch"))
            }
            Value::Array(items) => {
                let answers: Vec<Value> = items.into_iter().filter_map(|m| self.handle_one(&m, host)).collect();
                (!answers.is_empty()).then_some(Value::Array(answers))
            }
            msg => self.handle_one(&msg, host),
        }
    }

    fn handle_one(&mut self, msg: &Value, host: &mut dyn ToolHost) -> Option<Value> {
        let (id, method, params) = match classify(msg) {
            Err(answer) => return Some(answer),
            Ok(Incoming::Response) => return None,
            Ok(Incoming::Notification { method }) => {
                if method == "notifications/initialized" {
                    self.initialized = true;
                }
                return None;
            }
            Ok(Incoming::Request { id, method, params }) => (id, method, params),
        };
        Some(match method.as_str() {
            "initialize" => self.initialize(id, &params),
            "ping" => result_response(id, json!({})),
            "tools/list" => result_response(id, json!({ "tools": host.list_tools() })),
            "tools/call" => self.call(id, &params, host),
            other => error_response(id, codes::METHOD_NOT_FOUND, format!("Method not found: {other}")),
        })
    }

    fn initialize(&mut self, id: Value, params: &Value) -> Value {
        let Some(requested) = params.get("protocolVersion").and_then(Value::as_str) else {
            return error_response(id, codes::INVALID_PARAMS, "Invalid params: protocolVersion is required");
        };
        let version = negotiate_version(Some(requested));
        self.version = Some(version);
        let mut server_info = json!({ "name": self.info.name, "version": self.info.version });
        if has_structured_content(version) {
            server_info["title"] = Value::String(self.info.title.clone());
        }
        let mut result = json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": server_info,
        });
        if let Some(text) = &self.info.instructions {
            result["instructions"] = Value::String(text.clone());
        }
        result_response(id, result)
    }

    fn call(&mut self, id: Value, params: &Value, host: &mut dyn ToolHost) -> Value {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return error_response(id, codes::INVALID_PARAMS, "Invalid params: name is required");
        };
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => Value::Object(Map::new()),
            Some(Value::Object(m)) => Value::Object(m.clone()),
            Some(_) => {
                return result_response(
                    id,
                    ToolResult::error("arguments must be a JSON object").to_value(self.version()),
                );
            }
        };
        let version = self.version();
        match host.call_tool(name, arguments, version) {
            Some(result) => result_response(id, result.to_value(version)),
            None => error_response(id, codes::INVALID_PARAMS, format!("Unknown tool: {name}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl ToolHost for Echo {
        fn list_tools(&mut self) -> Vec<Value> {
            vec![json!({ "name": "echo", "description": "Echo", "inputSchema": { "type": "object" } })]
        }
        fn call_tool(&mut self, name: &str, arguments: Value, _version: &'static str) -> Option<ToolResult> {
            match name {
                "echo" => Some(ToolResult::json(arguments)),
                "fail" => Some(ToolResult::error("it failed")),
                _ => None,
            }
        }
    }

    fn session() -> Session {
        Session::new(ServerInfo::hammer_pro())
    }

    fn ask(s: &mut Session, text: &str) -> Value {
        s.handle_text(text, &mut Echo).expect("an answer")
    }

    #[test]
    fn initialize_negotiates_the_version() {
        for (asked, answered) in [
            ("2025-06-18", "2025-06-18"),
            ("2025-03-26", "2025-03-26"),
            ("2024-11-05", "2024-11-05"),
            ("2025-11-25", "2025-11-25"),
            ("2099-01-01", LATEST_VERSION),
            ("1.0", LATEST_VERSION),
        ] {
            let mut s = session();
            let r = ask(
                &mut s,
                &format!(
                    r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{asked}","capabilities":{{}},"clientInfo":{{"name":"t","version":"1"}}}}}}"#
                ),
            );
            assert_eq!(r["id"], 1);
            assert_eq!(r["result"]["protocolVersion"], answered, "{asked}");
            assert_eq!(r["result"]["serverInfo"]["name"], "hammer-pro");
            assert_eq!(r["result"]["capabilities"]["tools"]["listChanged"], false);
            assert!(r["result"]["instructions"].as_str().is_some_and(|t| t.contains("Hammer Pro")));
            assert_eq!(s.version(), answered);
        }
        let mut s = session();
        let r = ask(
            &mut s,
            r#"{"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        );
        assert_eq!(r["result"]["serverInfo"]["title"], "Hammer Pro");
        let r = ask(
            &mut s,
            r#"{"jsonrpc":"2.0","id":"b","method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        );
        assert!(r["result"]["serverInfo"].get("title").is_none());
        let r = ask(&mut s, r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}"#);
        assert_eq!(r["error"]["code"], codes::INVALID_PARAMS);
    }

    #[test]
    fn notifications_are_not_answered() {
        let mut s = session();
        assert!(s.handle_text(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &mut Echo).is_none());
        assert!(s.client_ready());
        assert!(
            s.handle_text(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{}}"#, &mut Echo).is_none()
        );
        assert!(s.handle_text(r#"{"jsonrpc":"2.0","id":5,"result":{}}"#, &mut Echo).is_none());
    }

    #[test]
    fn ping_tools_and_errors() {
        let mut s = session();
        let r = ask(&mut s, r#"{"jsonrpc":"2.0","id":"123","method":"ping"}"#);
        assert_eq!(r, json!({"jsonrpc":"2.0","id":"123","result":{}}));
        let r = ask(&mut s, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"cursor":"x"}}"#);
        assert_eq!(r["result"]["tools"][0]["name"], "echo");
        let r = ask(
            &mut s,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"a":1}}}"#,
        );
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(r["result"]["structuredContent"]["a"], 1);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("\"a\": 1"));
        let r = ask(&mut s, r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"fail"}}"#);
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(r["result"]["content"][0]["text"], "it failed");
        let r = ask(&mut s, r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope"}}"#);
        assert_eq!(r["error"]["code"], codes::INVALID_PARAMS);
        assert_eq!(r["error"]["message"], "Unknown tool: nope");
        let r =
            ask(&mut s, r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"echo","arguments":[1]}}"#);
        assert_eq!(r["result"]["isError"], true);
        let r = ask(&mut s, r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{}}"#);
        assert_eq!(r["error"]["code"], codes::INVALID_PARAMS);
        for method in ["server/discover", "resources/list", "prompts/list", "whatever"] {
            let r = ask(&mut s, &format!(r#"{{"jsonrpc":"2.0","id":8,"method":"{method}"}}"#));
            assert_eq!(r["error"]["code"], codes::METHOD_NOT_FOUND, "{method}");
            assert_eq!(r["id"], 8);
        }
    }

    #[test]
    fn malformed_messages() {
        let mut s = session();
        let r = ask(&mut s, "{not json");
        assert_eq!(r["error"]["code"], codes::PARSE_ERROR);
        assert_eq!(r["id"], Value::Null);
        let r = ask(&mut s, r#"{"id":1,"method":"ping"}"#);
        assert_eq!(r["error"]["code"], codes::INVALID_REQUEST);
        assert_eq!(r["id"], 1);
        let r = ask(&mut s, r#"{"jsonrpc":"2.0","id":{"x":1},"method":"ping"}"#);
        assert_eq!(r["error"]["code"], codes::INVALID_REQUEST);
        let r = ask(&mut s, r#"{"jsonrpc":"2.0","id":1,"method":7}"#);
        assert_eq!(r["error"]["code"], codes::INVALID_REQUEST);
        let r = ask(&mut s, "[]");
        assert_eq!(r["error"]["code"], codes::INVALID_REQUEST);
        let r = ask(&mut s, "42");
        assert_eq!(r["error"]["code"], codes::INVALID_REQUEST);
        let r = ask(
            &mut s,
            r#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"nope"}]"#,
        );
        assert_eq!(r.as_array().map(Vec::len), Some(2));
        assert_eq!(r[1]["error"]["code"], codes::METHOD_NOT_FOUND);
        let frame = Frame::TooLarge;
        let r = s.handle_frame(&frame, &mut Echo).unwrap();
        assert_eq!(r["error"]["code"], codes::INVALID_REQUEST);
    }

    #[test]
    fn structured_content_follows_the_version() {
        let r = ToolResult::json(json!({ "x": 1 }));
        assert!(r.to_value("2025-06-18").get("structuredContent").is_some());
        assert!(r.to_value("2025-11-25").get("structuredContent").is_some());
        assert!(r.to_value("2025-03-26").get("structuredContent").is_none());
        let png = ToolResult::png(&[1, 2, 3], "shot");
        let v = png.to_value(LATEST_VERSION);
        assert_eq!(v["content"][0]["type"], "image");
        assert_eq!(v["content"][0]["mimeType"], "image/png");
        assert_eq!(v["content"][0]["data"], "AQID");
        assert_eq!(ToolResult::from_value(&v), Some(png));
    }

    #[test]
    fn framing_limits_and_lines() {
        let data = b"{\"a\":1}\r\n\n   \n{\"b\":2}\n".to_vec();
        let mut r = io::Cursor::new(data);
        assert_eq!(read_message(&mut r, 100).unwrap(), Frame::Message("{\"a\":1}".into()));
        assert_eq!(read_message(&mut r, 100).unwrap(), Frame::Message("{\"b\":2}".into()));
        assert_eq!(read_message(&mut r, 100).unwrap(), Frame::Eof);

        let mut big = vec![b'x'; 5000];
        big.extend_from_slice(b"\n{\"ok\":true}\n");
        let mut r = io::BufReader::with_capacity(64, io::Cursor::new(big));
        assert_eq!(read_message(&mut r, 1000).unwrap(), Frame::TooLarge);
        assert_eq!(read_message(&mut r, 1000).unwrap(), Frame::Message("{\"ok\":true}".into()));
        let mut r = io::Cursor::new(b"12345\n".to_vec());
        assert_eq!(read_message(&mut r, 5).unwrap(), Frame::Message("12345".into()));
        let mut r = io::Cursor::new(b"{}".to_vec());
        assert_eq!(read_message(&mut r, 5).unwrap(), Frame::Message("{}".into()));
        let mut r = io::Cursor::new(vec![0xff, 0xfe, b'\n']);
        assert_eq!(read_message(&mut r, 5).unwrap(), Frame::NotUtf8);

        let mut out = Vec::new();
        write_message(&mut out, &json!({ "text": "a\nb" })).unwrap();
        assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), 1);
        assert!(out.ends_with(b"\n"));
    }
}
