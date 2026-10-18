//! `fastanim lsp`: a language server for scene scripts. It bakes each open script as it changes
//! and reports the error, and completes and describes the scene API from
//! [`fastanim_script::signatures`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use lsp_server::{Connection, ErrorCode, Message, Notification, Response};
use serde_json::{Value, json};

/// Serves LSP on stdin and stdout until the client shuts it down.
pub fn run() -> Result<(), String> {
    let (conn, io) = Connection::stdio();
    serve(&conn).map_err(|e| e.to_string())?;
    io.join().map_err(|e| e.to_string())
}

fn serve(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    conn.initialize(json!({
        "textDocumentSync": 1, // whole documents
        "completionProvider": {},
        "hoverProvider": true,
    }))?;
    let api = api();
    let mut docs = HashMap::<String, String>::new();
    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    break;
                }
                let resp = match req.method.as_str() {
                    "textDocument/completion" | "textDocument/hover" => {
                        let doc = (req.params["textDocument"]["uri"].as_str())
                            .and_then(|uri| docs.get(uri))
                            .map_or("", String::as_str);
                        let names = with_declared(&api, doc);
                        let pos = &req.params["position"];
                        let result = match req.method.as_str() {
                            "textDocument/completion" => completion(&names),
                            _ => json!(
                                word_at(
                                    doc,
                                    pos["line"].as_u64().unwrap_or(0),
                                    pos["character"].as_u64().unwrap_or(0)
                                )
                                .and_then(|w| hover(&names, w))
                            ),
                        };
                        Response::new_ok(req.id, result)
                    }
                    _ => Response::new_err(
                        req.id,
                        ErrorCode::MethodNotFound as i32,
                        format!("unsupported: {}", req.method),
                    ),
                };
                conn.sender.send(resp.into())?;
            }
            Message::Notification(n) => {
                let Some(uri) = n.params["textDocument"]["uri"].as_str() else {
                    continue;
                };
                let text = match n.method.as_str() {
                    "textDocument/didOpen" => n.params["textDocument"]["text"].as_str(),
                    "textDocument/didChange" => n.params["contentChanges"]
                        .as_array()
                        .and_then(|c| c.last()?["text"].as_str()),
                    "textDocument/didClose" => {
                        docs.remove(uri);
                        publish(conn, uri, vec![])?;
                        continue;
                    }
                    _ => continue,
                };
                if let Some(text) = text {
                    publish(conn, uri, diagnostics(uri, text))?;
                    docs.insert(uri.to_string(), text.to_string());
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn publish(
    conn: &Connection,
    uri: &str,
    diagnostics: Vec<Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let params = json!({ "uri": uri, "diagnostics": diagnostics });
    let n = Notification::new("textDocument/publishDiagnostics".into(), params);
    Ok(conn.sender.send(n.into())?)
}

/// The error baking `text` gives, if any, with relative paths in it relative to its file's
/// directory.
fn diagnostics(uri: &str, text: &str) -> Vec<Value> {
    let dir = uri_path(uri).and_then(|p| Some(p.parent()?.to_path_buf()));
    let Err(e) = fastanim_script::bake_in(text, &dir.unwrap_or_default()) else {
        return vec![];
    };
    // shortcut: Rhai counts columns in chars, LSP in UTF-16 units; they differ only after
    // astral characters (emoji) on the line.
    let (line, col) = (e.line.saturating_sub(1), e.col.saturating_sub(1));
    vec![json!({
        "range": {
            "start": { "line": line, "character": col },
            "end": { "line": line, "character": col + 1 },
        },
        "severity": 1,
        "source": "fastanim",
        "message": e.message,
    })]
}

/// The path of a `file:` URI, percent-decoded.
fn uri_path(uri: &str) -> Option<PathBuf> {
    let s = uri.strip_prefix("file://")?.as_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let hex = s
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(b) if s[i] == b'%' => {
                out.push(b);
                i += 3;
            }
            _ => {
                out.push(s[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok().map(PathBuf::from)
}

/// Names a script can use, each with its LSP completion kind and its signatures or, for names
/// the script declares, the lines declaring them.
type Names = BTreeMap<String, (u8, Vec<String>)>;

const FUNCTION: u8 = 3;
const VARIABLE: u8 = 6;
const CONSTANT: u8 = 21;

/// Each function and constant scripts can use, and `scene`.
fn api() -> Names {
    let mut api = Names::new();
    api.insert("scene".into(), (VARIABLE, vec!["scene: Scene".into()]));
    for sig in fastanim_script::signatures() {
        let (kind, name) = match sig.strip_prefix("const ") {
            Some(name) => (CONSTANT, name),
            None => (FUNCTION, sig.split('(').next().unwrap_or(&sig)),
        };
        // Operators can't be completed by name.
        if is_ident(name) {
            let entry = api.entry(name.to_string()).or_insert((kind, vec![]));
            entry.1.push(sig);
        }
    }
    api
}

/// `api` with the names `text` declares, which shadow it.
fn with_declared(api: &Names, text: &str) -> Names {
    let mut names = api.clone();
    let mut shadowed = HashSet::new();
    for (name, kind, line) in declared(text) {
        let entry = names.entry(name.to_string()).or_default();
        if shadowed.insert(name) {
            *entry = (kind, vec![]);
        }
        if !entry.1.iter().any(|l| l == line) {
            entry.1.push(line.to_string());
        }
    }
    names
}

/// The names `text` declares, with their kinds and the lines declaring them: `let`, `const`,
/// `for` and `fn` names, and `fn` and closure parameters.
/// shortcut: scans the text rather than parsing it, so it works on a half-typed script but also
/// picks up `let x` in strings and comments; parse with Rhai if that misleads.
fn declared(text: &str) -> Vec<(&str, u8, &str)> {
    let mut out = vec![];
    for line in text.lines() {
        let decl = line.trim();
        let words: Vec<&str> = (line.split(|c: char| !(c.is_alphanumeric() || c == '_')))
            .filter(|w| !w.is_empty())
            .collect();
        for w in words.windows(2) {
            let kind = match w[0] {
                "let" | "for" => VARIABLE,
                "const" => CONSTANT,
                "fn" => FUNCTION,
                _ => continue,
            };
            out.push((w[1], kind, decl));
        }
        // `fn f(a, b)` and `for (x, i) in`.
        let after = |kw| decl.strip_prefix(kw)?.split_once('(')?.1.split_once(')');
        if let Some((list, _)) = after("fn ").or_else(|| after("for ")) {
            out.extend(
                params(list)
                    .into_iter()
                    .flatten()
                    .map(|p| (p, VARIABLE, decl)),
            );
        }
        // `|a, b|`: every other piece between pipes.
        for piece in line.split('|').skip(1).step_by(2) {
            out.extend(
                params(piece)
                    .into_iter()
                    .flatten()
                    .map(|p| (p, VARIABLE, decl)),
            );
        }
    }
    out
}

/// The names in a comma-separated parameter list, if it is one.
fn params(list: &str) -> Option<Vec<&str>> {
    let ps: Vec<&str> = list.split(',').map(str::trim).collect();
    ps.iter().all(|p| is_ident(p)).then_some(ps)
}

fn is_ident(s: &str) -> bool {
    s.starts_with(|c: char| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn completion(names: &Names) -> Value {
    (names.iter())
        .map(|(name, (kind, sigs))| {
            json!({
                "label": name,
                "kind": kind,
                "detail": sigs[0],
                "documentation": sigs.join("\n"),
            })
        })
        .collect()
}

fn hover(names: &Names, word: &str) -> Option<Value> {
    let (_, sigs) = names.get(word)?;
    Some(json!({
        "contents": { "kind": "markdown", "value": format!("```rhai\n{}\n```", sigs.join("\n")) },
    }))
}

/// The identifier around 0-based `line` and `character`.
fn word_at(text: &str, line: u64, character: u64) -> Option<&str> {
    let line = text.lines().nth(line as usize)?;
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    // Byte offset of the cursor, clamped to the line's end.
    let at = (line.char_indices().nth(character as usize)).map_or(line.len(), |(i, _)| i);
    let start = line[..at].rfind(|c| !ident(c)).map_or(0, |i| i + 1);
    let end = line[at..]
        .find(|c| !ident(c))
        .map_or(line.len(), |i| at + i);
    (start < end).then(|| &line[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        assert_eq!(word_at("let c = circle(1.0);", 0, 10), Some("circle"));
        assert_eq!(word_at("let c = circle(1.0);", 0, 14), Some("circle"));
        assert_eq!(word_at("x\n  BLUE", 1, 6), Some("BLUE"));
        assert_eq!(word_at("a + b", 0, 2), None);
    }

    #[test]
    fn declarations() {
        let text = "let c = circle(1.0);\nconst K = 2.0;\nfn f(a, b) { a }\nfor (x, i) in xs {}\nscene.always(c, |state, t| state);\nlet n = a || b | 3;";
        let names: Vec<_> = declared(text).iter().map(|d| (d.0, d.1)).collect();
        assert_eq!(
            names,
            [
                ("c", VARIABLE),
                ("K", CONSTANT),
                ("f", FUNCTION),
                ("a", VARIABLE),
                ("b", VARIABLE)
            ]
            .into_iter()
            .chain([("x", VARIABLE), ("x", VARIABLE), ("i", VARIABLE)])
            .chain([("state", VARIABLE), ("t", VARIABLE), ("n", VARIABLE)])
            .collect::<Vec<_>>()
        );
        // A script's `circle` shadows the API's.
        let names = with_declared(&api(), "fn circle(r) {}");
        assert_eq!(
            names["circle"],
            (FUNCTION, vec!["fn circle(r) {}".to_string()])
        );
        assert_eq!(names["scene"].0, VARIABLE);
    }

    #[test]
    fn uris() {
        assert_eq!(
            uri_path("file:///home/a%20b/s.rhai"),
            Some(PathBuf::from("/home/a b/s.rhai"))
        );
        assert_eq!(uri_path("untitled:Untitled-1"), None);
    }

    #[test]
    fn session() {
        let (client, server) = Connection::memory();
        let thread = std::thread::spawn(move || serve(&server).unwrap());
        let send = |m: Message| client.sender.send(m).unwrap();
        let request = |id: i32, method: &str, params: Value| {
            send(lsp_server::Request::new(id.into(), method.into(), params).into());
            match client.receiver.recv().unwrap() {
                Message::Response(r) => r.response_result.unwrap(),
                m => panic!("expected a response, got {m:?}"),
            }
        };
        let notify = |method: &str, params: Value| {
            send(Notification::new(method.into(), params).into());
            match client.receiver.recv().unwrap() {
                Message::Notification(n) => n.params["diagnostics"].clone(),
                m => panic!("expected diagnostics, got {m:?}"),
            }
        };

        assert!(request(1, "initialize", json!({ "capabilities": {} }))["capabilities"]["hoverProvider"].as_bool().unwrap());
        send(Notification::new("initialized".into(), json!({})).into());

        let uri = "file:///nowhere/s.rhai";
        let open = |text: &str| json!({ "textDocument": { "uri": uri, "text": text } });
        let diags = notify(
            "textDocument/didOpen",
            open("let c = circle(1.0);\nscene.add(c);\nscene.wait(nope);"),
        );
        assert_eq!(
            diags[0]["range"]["start"],
            json!({ "line": 2, "character": 11 }),
            "{diags}"
        );
        let change = json!({ "textDocument": { "uri": uri }, "contentChanges": [{ "text": "let c = circle(1.0);" }] });
        assert_eq!(notify("textDocument/didChange", change), json!([]));

        let hovered = request(
            2,
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 10 } }),
        );
        assert!(
            hovered["contents"]["value"]
                .as_str()
                .unwrap()
                .contains("circle("),
            "{hovered}"
        );
        let items = request(3, "textDocument/completion", json!({}));
        assert!(
            items
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["label"] == "BLUE")
        );

        request(4, "shutdown", Value::Null);
        send(Notification::new("exit".into(), Value::Null).into());
        thread.join().unwrap();
    }
}
