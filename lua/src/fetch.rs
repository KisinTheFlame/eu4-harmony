//! `http.fetch(url, options)`, the `network` sensitive capability
//! (ADR sensitive-capabilities-are-declared): an HTTP or HTTPS request to a
//! host the manifest lists, which the host sends while the task that sent it
//! waits (ADR handlers-run-as-tasks-that-can-wait). Here: what the manifest
//! allows, what the options ask for, and the answer as Lua gets it.
//!
//! The host follows no redirect, so a request never reaches a host the
//! manifest does not list: a 3xx is a response like any other.
use mlua::{Lua, MultiValue, Table, Value as LuaValue};
use std::time::Duration;

/// `timeout` when left out, and the most it may be.
const TIMEOUT: Duration = Duration::from_secs(30);
const LONGEST: Duration = Duration::from_secs(300);

/// The largest body a response may have.
pub const LARGEST: usize = 16 << 20;

/// Where a URL sends its request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub secure: bool,
    /// A name or an address, lowercased; an IPv6 address in brackets.
    pub host: String,
    pub port: u16,
    /// The path and query, `/` at least; no fragment.
    pub path: String,
}

impl Target {
    /// The URL to send to, with its port written out.
    pub fn url(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        format!("{scheme}://{}:{}{}", self.host, self.port, self.path)
    }
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

/// A request the manifest allows, as `options` asks for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub target: Target,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timeout: Duration,
}

/// The request to `target` that `options` describes: `method`, `headers`,
/// `body`, `timeout`.
pub fn request(target: Target, options: Option<Table>) -> Result<Request, String> {
    let mut request = Request { target, method: "GET".into(), headers: Vec::new(), body: Vec::new(), timeout: TIMEOUT };
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u32,
    /// Lowercased names; a header given more than once joined with `, `.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Why no response came.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Timeout,
    Network(String),
    TooLarge,
}

/// What a request comes to.
pub type Answer = Result<Response, Failure>;

/// The answer as `http.fetch` returns it: `response`, or `nil, err`.
pub fn answer(lua: &Lua, answer: Answer) -> mlua::Result<MultiValue> {
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
