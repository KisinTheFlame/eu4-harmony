//! `http.fetch(url, options)`, the `network` sensitive capability (ADR 0025):
//! an HTTP or HTTPS request to a host the manifest lists, sent by reqwest
//! while the task that sent it waits (ADR 0030). Every extension's requests
//! go from one thread of the framework's.
//!
//! Redirects are not followed, so a request never reaches a host the
//! manifest does not list: a 3xx is a response like any other. The system's
//! proxy settings are used, and its certificates, through SChannel.
use mlua::{Lua, MultiValue, Table, Value as LuaValue};
use std::{
    sync::{
        mpsc::{self, Receiver, TryRecvError},
        OnceLock,
    },
    time::{Duration, Instant},
};

/// `timeout` when left out, and the most it may be.
const TIMEOUT: Duration = Duration::from_secs(30);
const LONGEST: Duration = Duration::from_secs(300);

/// The largest body a response may have.
pub const LARGEST: usize = 16 << 20;

/// Where a URL sends its request.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    secure: bool,
    host: String,
    port: u16,
    /// The path and query, `/` at least; no fragment.
    path: String,
}

/// A URL's target: `http://` or `https://`, a host, and the rest.
pub fn target(url: &str) -> Result<Target, String> {
    let wrong = |why: &str| format!("{url:?} is not a URL: {why}");
    let (secure, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        _ => return Err(wrong("it begins with http:// or https://")),
    };
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(wrong("it has a space or a control character, which is written %20 and the like"));
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.contains('@') {
        return Err(wrong("a user name in a URL is not sent"));
    }
    let (host, port) = eu4_harmony_manifest::host(authority).map_err(|e| wrong(&e))?;
    let tail = tail.split('#').next().unwrap_or_default();
    let path = match tail {
        "" => "/".to_string(),
        t if t.starts_with('?') => format!("/{t}"),
        t => t.to_string(),
    };
    Ok(Target { secure, port: port.unwrap_or(if secure { 443 } else { 80 }), host, path })
}

/// Whether the manifest's `network` lists the target's host: by name or
/// address, and by port, which is the scheme's default where an entry has
/// none.
pub fn listed(target: &Target, hosts: &[String]) -> bool {
    let default = if target.secure { 443 } else { 80 };
    hosts.iter().filter_map(|h| eu4_harmony_manifest::host(h).ok()).any(|(host, port)| host == target.host && port.unwrap_or(default) == target.port)
}

/// What `options` asks for.
pub struct Request {
    method: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    timeout: Duration,
}

/// The request `options` describes: `method`, `headers`, `body`, `timeout`.
pub fn request(options: Option<Table>) -> Result<Request, String> {
    let mut request = Request { method: "GET".into(), headers: Vec::new(), body: Vec::new(), timeout: TIMEOUT };
    let Some(options) = options else { return Ok(request) };
    let token = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c));
    for pair in options.pairs::<String, LuaValue>() {
        let (key, value) = pair.map_err(|e| e.to_string())?;
        match (key.as_str(), value) {
            ("method", LuaValue::String(s)) => {
                let method = s.to_str().map_err(|e| e.to_string())?.to_ascii_uppercase();
                if !token(&method) {
                    return Err(format!("method {method:?} is not an HTTP method"));
                }
                request.method = method;
            }
            ("headers", LuaValue::Table(t)) => {
                for pair in t.pairs::<String, String>() {
                    let (name, value) = pair.map_err(|_| "headers is a table of names to strings".to_string())?;
                    if !token(&name) || value.contains(['\r', '\n']) {
                        return Err(format!("header {name:?} is not a header name with a value on one line"));
                    }
                    request.headers.push((name, value));
                }
            }
            ("body", LuaValue::String(s)) => request.body = s.as_bytes().to_vec(),
            ("timeout", LuaValue::Integer(n)) => request.timeout = timeout(n as f64)?,
            ("timeout", LuaValue::Number(n)) => request.timeout = timeout(n)?,
            ("method" | "headers" | "body" | "timeout", other) => {
                let wants = match key.as_str() {
                    "headers" => "a table",
                    "timeout" => "a number of seconds",
                    _ => "a string",
                };
                return Err(format!("{key} is {wants}, not a {}", other.type_name()));
            }
            (other, _) => return Err(format!("has no option {other}; it has method, headers, body and timeout")),
        }
    }
    request.headers.sort();
    Ok(request)
}

fn timeout(seconds: f64) -> Result<Duration, String> {
    if seconds > 0.0 && seconds <= LONGEST.as_secs_f64() {
        Ok(Duration::from_secs_f64(seconds))
    } else {
        Err(format!("timeout is more than 0 and at most {} seconds, not {seconds}", LONGEST.as_secs()))
    }
}

/// A response, whatever its status.
#[derive(Debug)]
pub struct Response {
    status: u32,
    /// Lowercased names; a header given more than once joined with `, `.
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// Why no response came.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Timeout,
    Network(String),
    TooLarge,
}

/// A request on its way.
pub struct Pending {
    answer: Receiver<Result<Response, Failure>>,
    deadline: Instant,
}

impl Pending {
    /// Its answer, once it has come or its time is up.
    pub fn poll(&mut self) -> Option<Result<Response, Failure>> {
        match self.answer.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Disconnected) => Some(Err(Failure::Network("the request ended without an answer".into()))),
            Err(TryRecvError::Empty) if Instant::now() >= self.deadline => Some(Err(Failure::Timeout)),
            Err(TryRecvError::Empty) => None,
        }
    }
}

/// Sends the request from the framework's thread for them.
pub fn start(target: Target, request: Request) -> Pending {
    let (tell, answer) = mpsc::channel();
    let deadline = Instant::now() + request.timeout;
    match sender() {
        Ok((runtime, client)) => {
            runtime.spawn(async move {
                let _ = tell.send(send(client, &target, &request).await);
            });
        }
        Err(e) => {
            let _ = tell.send(Err(Failure::Network(format!("no request can be sent: {e}"))));
        }
    }
    Pending { answer, deadline }
}

/// The answer as `http.fetch` returns it: `response`, or `nil, err`.
pub fn answer(lua: &Lua, answer: Result<Response, Failure>) -> mlua::Result<MultiValue> {
    match answer {
        Ok(response) => {
            let headers = lua.create_table()?;
            for (name, value) in response.headers {
                headers.set(name, value)?;
            }
            let table = lua.create_table()?;
            table.set("status", response.status)?;
            table.set("headers", headers)?;
            table.set("body", lua.create_string(response.body)?)?;
            Ok(MultiValue::from_iter([LuaValue::Table(table)]))
        }
        Err(failure) => {
            let (code, message) = match failure {
                Failure::Timeout => ("timeout", "no answer came in time".to_string()),
                Failure::Network(message) => ("network", message),
                Failure::TooLarge => ("too_large", format!("the response's body is larger than {} MiB", LARGEST >> 20)),
            };
            let err = lua.create_table()?;
            err.set("code", code)?;
            err.set("message", message)?;
            Ok(MultiValue::from_iter([LuaValue::Nil, LuaValue::Table(err)]))
        }
    }
}

// ---------- sending ----------

/// The thread requests are sent from, as a tokio runtime, and the client
/// that sends them: made with the first request.
fn sender() -> Result<&'static (tokio::runtime::Handle, reqwest::Client), &'static str> {
    static SENDER: OnceLock<Result<(tokio::runtime::Handle, reqwest::Client), String>> = OnceLock::new();
    let made = SENDER.get_or_init(|| {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        let handle = runtime.handle().clone();
        let client = {
            let _inside = handle.enter();
            reqwest::Client::builder()
                .user_agent(concat!("EU4 Harmony/", env!("CARGO_PKG_VERSION")))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| e.to_string())?
        };
        std::thread::Builder::new().name("eu4harmony-http".into()).spawn(move || runtime.block_on(std::future::pending::<()>())).map_err(|e| e.to_string())?;
        Ok((handle, client))
    });
    made.as_ref().map_err(String::as_str)
}

/// Sends the request and reads its response.
async fn send(client: &reqwest::Client, target: &Target, request: &Request) -> Result<Response, Failure> {
    let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| Failure::Network(e.to_string()))?;
    let scheme = if target.secure { "https" } else { "http" };
    let url = format!("{scheme}://{}:{}{}", target.host, target.port, target.path);
    let mut sending = client.request(method, url).timeout(request.timeout).body(request.body.clone());
    for (name, value) in &request.headers {
        sending = sending.header(name, value);
    }
    let mut response = sending.send().await.map_err(failure)?;
    let status = u32::from(response.status().as_u16());
    let mut headers: Vec<(String, String)> = Vec::new();
    for (name, value) in response.headers() {
        let value = String::from_utf8_lossy(value.as_bytes());
        match headers.iter_mut().find(|(n, _)| n == name.as_str()) {
            Some((_, joined)) => {
                joined.push_str(", ");
                joined.push_str(&value);
            }
            None => headers.push((name.as_str().to_string(), value.into_owned())),
        }
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(failure)? {
        if body.len() + chunk.len() > LARGEST {
            return Err(Failure::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Response { status, headers, body })
}

/// Why reqwest got no response, with the causes it gives.
fn failure(e: reqwest::Error) -> Failure {
    if e.is_timeout() {
        return Failure::Timeout;
    }
    let mut message = if e.is_connect() { "the host cannot be reached".to_string() } else { "the request failed".to_string() };
    let mut cause = std::error::Error::source(&e);
    while let Some(c) = cause {
        message.push_str(": ");
        message.push_str(&c.to_string());
        cause = c.source();
    }
    Failure::Network(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_names_its_scheme_host_port_and_path() {
        let t = |secure, host: &str, port, path: &str| Target { secure, host: host.into(), port, path: path.into() };
        assert_eq!(target("https://API.example.com").unwrap(), t(true, "api.example.com", 443, "/"));
        assert_eq!(target("http://192.168.1.10:8080/score?x=1#top").unwrap(), t(false, "192.168.1.10", 8080, "/score?x=1"));
        assert_eq!(target("http://[::1]:3000?q").unwrap(), t(false, "[::1]", 3000, "/?q"));
        for bad in ["ftp://x.com", "x.com", "https://", "https://u:p@x.com/", "https://x.com/a b", "https://x.com:99999/"] {
            assert!(target(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_host_is_listed_by_name_and_port() {
        let hosts = ["api.example.com".to_string(), "192.168.1.10:8080".to_string()];
        assert!(listed(&target("https://api.example.com/x").unwrap(), &hosts));
        assert!(listed(&target("http://api.example.com/x").unwrap(), &hosts));
        assert!(!listed(&target("https://api.example.com:8443/x").unwrap(), &hosts));
        assert!(!listed(&target("https://example.com/x").unwrap(), &hosts));
        assert!(listed(&target("http://192.168.1.10:8080/").unwrap(), &hosts));
        assert!(!listed(&target("http://192.168.1.10/").unwrap(), &hosts));
    }
}
