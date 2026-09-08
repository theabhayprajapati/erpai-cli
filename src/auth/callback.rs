use crate::error::{CliError, Result};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Callback {
    pub code: String,
    pub apps: Vec<String>,
}

pub struct Listener {
    inner: TcpListener,
}

impl Listener {
    pub fn bind() -> Result<(Self, u16)> {
        let l = TcpListener::bind(("127.0.0.1", 0))?;
        let port = l.local_addr()?.port();
        l.set_nonblocking(true)?;
        Ok((Self { inner: l }, port))
    }

    /// Wait for exactly one GET /callback?code=&state=&apps= whose state matches. Blocking, with a deadline.
    pub fn wait(&self, expected_state: &str, timeout: Duration) -> Result<Callback> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.inner.accept() {
                Ok((stream, _)) => {
                    if let Some(cb) = handle(stream, expected_state)? {
                        return Ok(cb);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() > deadline {
                        return Err(CliError::auth("timed out waiting for the browser sign-in")
                            .with_hint(
                                "run `erpai login` again and complete the sign-in within 5 minutes",
                            ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

fn handle(mut stream: TcpStream, expected_state: &str) -> Result<Option<Callback>> {
    // accepted sockets inherit the listener's non-blocking flag on some platforms; read synchronously
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]);
    let line = req.lines().next().unwrap_or("");
    let target = line.split_whitespace().nth(1).unwrap_or("");
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| CliError::auth("malformed callback request"))?;
    if url.path() != "/callback" {
        respond(&mut stream, 404, "not found");
        return Ok(None);
    }
    let get = |k: &str| {
        url.query_pairs()
            .find(|(a, _)| a == k)
            .map(|(_, v)| v.to_string())
    };
    if get("state").as_deref() != Some(expected_state) {
        respond(
            &mut stream,
            400,
            "state mismatch — start `erpai login` again",
        );
        return Ok(None);
    }
    let code = match get("code") {
        Some(c) if !c.is_empty() => c,
        _ => {
            respond(&mut stream, 400, "missing code");
            return Ok(None);
        }
    };
    let apps = get("apps")
        .map(|a| {
            a.split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    respond(
        &mut stream,
        200,
        "Signed in to ERP AI — you can close this tab and return to your terminal.",
    );
    Ok(Some(Callback { code, apps }))
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>erpai</title><body style=\"font-family:system-ui;padding:2rem\"><p>{body}</p></body>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_state_and_parses_apps() {
        let (l, port) = Listener::bind().unwrap();
        let t = std::thread::spawn(move || l.wait("S", Duration::from_secs(5)));
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "GET /callback?code=C&state=S&apps=a1,a2 HTTP/1.1\r\nHost: x\r\n\r\n"
        )
        .unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(resp.starts_with("HTTP/1.1 200"));
        let cb = t.join().unwrap().unwrap();
        assert_eq!(cb.code, "C");
        assert_eq!(cb.apps, vec!["a1", "a2"]);
    }

    #[test]
    fn rejects_wrong_state_and_keeps_waiting() {
        let (l, port) = Listener::bind().unwrap();
        let t = std::thread::spawn(move || l.wait("S", Duration::from_millis(800)));
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET /callback?code=C&state=WRONG HTTP/1.1\r\n\r\n").unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(resp.starts_with("HTTP/1.1 400"));
        assert_eq!(t.join().unwrap().unwrap_err().code.as_str(), "auth_error");
    }
}
