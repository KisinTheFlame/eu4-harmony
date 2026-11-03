//! Sends `http.fetch`'s requests for the live host: reqwest with native-tls
//! (SChannel, the system's certificates) and the system's proxy, from one
//! thread of the framework's for every extension. Redirects are not
//! followed, so a request never reaches a host the manifest does not list.
use eu4_harmony_lua::{
    fetch::{Answer, Failure, Request, Response, LARGEST},
    Pending,
};
use std::sync::{
    mpsc::{self, Receiver, TryRecvError},
    OnceLock,
};

/// A request on its way.
struct OnItsWay(Receiver<Answer>);

impl Pending<Answer> for OnItsWay {
    fn poll(&mut self) -> Option<Answer> {
        match self.0.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Disconnected) => Some(Err(Failure::Network("the request ended without an answer".into()))),
            Err(TryRecvError::Empty) => None,
        }
    }
}

/// Sends `request` from the framework's thread for them.
pub fn start(request: Request) -> Box<dyn Pending<Answer>> {
    let (tell, answer) = mpsc::channel();
    match sender() {
        Ok((runtime, client)) => {
            runtime.spawn(async move {
                let _ = tell.send(send(client, &request).await);
            });
        }
        Err(e) => {
            let _ = tell.send(Err(Failure::Network(format!("no request can be sent: {e}"))));
        }
    }
    Box::new(OnItsWay(answer))
}

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
async fn send(client: &reqwest::Client, request: &Request) -> Answer {
    let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| Failure::Network(e.to_string()))?;
    let mut sending = client.request(method, request.target.url()).timeout(request.timeout).body(request.body.clone());
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
    use std::time::{Duration, Instant};

    /// A server on this machine that answers one request with `answer` (or
    /// never, for none) and tells what it was asked.
    fn serve_once(answer: Option<&'static str>) -> (u16, mpsc::Receiver<String>) {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tell, asked) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buffer) {
                request.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()));
                    if body.len() >= length.unwrap_or(0) {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let _ = tell.send(String::from_utf8_lossy(&request).to_string());
            match answer {
                Some(answer) => {
                    let _ = stream.write_all(answer.as_bytes());
                }
                None => std::thread::sleep(Duration::from_secs(5)),
            }
        });
        (port, asked)
    }

    fn request(port: u16, path: &str, timeout: Duration) -> Request {
        let target = eu4_harmony_lua::fetch::Target { secure: false, host: "127.0.0.1".into(), port, path: path.into() };
        Request { target, method: "GET".into(), headers: Vec::new(), body: Vec::new(), timeout }
    }

    /// The answer, once it has come.
    fn wait(mut pending: Box<dyn Pending<Answer>>) -> Answer {
        let started = Instant::now();
        loop {
            if let Some(answer) = pending.poll() {
                return answer;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "no answer");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_request_is_sent_and_its_response_read() {
        let (port, asked) =
            serve_once(Some("HTTP/1.1 201 Created\r\nX-Rank: 3\r\nSet-Cookie: a=1\r\nset-cookie: b=2\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"));
        let mut post = request(port, "/score?x=1", Duration::from_secs(5));
        post.method = "POST".into();
        post.headers = vec![("Content-Type".into(), "application/json".into())];
        post.body = b"{\"a\":1}".to_vec();
        let response = wait(start(post)).unwrap();
        assert_eq!((response.status, response.body.as_slice()), (201, &b"hello"[..]));
        assert!(response.headers.contains(&("x-rank".into(), "3".into())) && response.headers.contains(&("set-cookie".into(), "a=1, b=2".into())));
        let asked = asked.recv().unwrap();
        assert!(asked.starts_with("POST /score?x=1 HTTP/1.1\r\n"), "{asked}");
        // Header names are sent in lowercase, which HTTP allows.
        assert!(asked.to_ascii_lowercase().contains("content-type: application/json\r\n") && asked.ends_with("{\"a\":1}"), "{asked}");
    }

    #[test]
    fn a_redirect_is_a_response_and_not_followed() {
        let (port, _asked) = serve_once(Some("HTTP/1.1 302 Found\r\nLocation: http://example.com/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"));
        let response = wait(start(request(port, "/", Duration::from_secs(5)))).unwrap();
        assert_eq!(response.status, 302);
        assert!(response.headers.contains(&("location".into(), "http://example.com/".into())));
    }

    #[test]
    fn a_request_with_no_answer_in_time_fails_with_timeout() {
        let (port, _asked) = serve_once(None);
        assert_eq!(wait(start(request(port, "/", Duration::from_millis(300)))), Err(Failure::Timeout));
    }
}
