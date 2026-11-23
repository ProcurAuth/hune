//! `hune health` (14.5): one `GET /livez` on the ops listener, with one deadline of 2 s.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// The deadline that covers the connection and the answer (14.5).
pub const DEADLINE: Duration = Duration::from_secs(2);

/// Replaces a wildcard host with the loopback address of the same family (14.5).
pub fn target_address(ops_listen: SocketAddr) -> SocketAddr {
    let ip = match ops_listen.ip() {
        IpAddr::V4(a) if a.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(a) if a.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        other => other,
    };
    SocketAddr::new(ip, ops_listen.port())
}

/// Reads the status code of an HTTP/1.1 answer from its first bytes.
pub fn parse_status(head: &[u8]) -> Option<u16> {
    let line_end = head.iter().position(|b| *b == b'\r' || *b == b'\n')?;
    let line = std::str::from_utf8(&head[..line_end]).ok()?;
    let mut parts = line.split(' ');
    if !parts.next()?.starts_with("HTTP/1.") {
        return None;
    }
    parts.next()?.parse().ok()
}

/// Sends `GET /livez` to the address. It gives `Ok` for the status 200 and a message for the other cases.
pub fn check(addr: SocketAddr, deadline: Duration) -> Result<(), String> {
    let start = Instant::now();
    let remaining = |what: &str| {
        deadline
            .checked_sub(start.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| format!("no answer from {addr} within {} ms ({what})", deadline.as_millis()))
    };
    let mut stream = TcpStream::connect_timeout(&addr, remaining("connect")?)
        .map_err(|e| format!("cannot connect to {addr}: {e}"))?;
    stream.set_write_timeout(Some(remaining("send")?)).map_err(|e| e.to_string())?;
    let request = format!("GET /livez HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).map_err(|e| format!("cannot send the request to {addr}: {e}"))?;
    let mut head = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        stream.set_read_timeout(Some(remaining("read")?)).map_err(|e| e.to_string())?;
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                head.extend_from_slice(&buf[..n]);
                if head.contains(&b'\n') || head.len() >= 1024 {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                return Err(format!("no answer from {addr} within {} ms", deadline.as_millis()));
            }
            Err(e) => return Err(format!("cannot read the answer of {addr}: {e}")),
        }
    }
    match parse_status(&head) {
        Some(200) => Ok(()),
        Some(other) => Err(format!("{addr} answered the status {other} to /livez")),
        None => Err(format!("{addr} gave an answer that is not HTTP/1.1")),
    }
}

/// Runs `hune health`. It writes nothing on success and one line on the standard error on a
/// failure. It gives the exit code: 0 or 1.
pub fn run_health(ops_listen: SocketAddr) -> u8 {
    match check(target_address(ops_listen), DEADLINE) {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("hune health: {message}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(answer: &'static [u8]) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
                let _ = s.write_all(answer);
            }
        });
        addr
    }

    #[test]
    fn wildcard_becomes_loopback() {
        assert_eq!(target_address("0.0.0.0:9090".parse().unwrap()), "127.0.0.1:9090".parse().unwrap());
        assert_eq!(target_address("[::]:9090".parse().unwrap()), "[::1]:9090".parse().unwrap());
        assert_eq!(target_address("10.0.0.5:9090".parse().unwrap()), "10.0.0.5:9090".parse().unwrap());
    }

    #[test]
    fn status_parse() {
        assert_eq!(parse_status(b"HTTP/1.1 200 OK\r\n"), Some(200));
        assert_eq!(parse_status(b"HTTP/1.1 503 Service Unavailable\r\n"), Some(503));
        assert_eq!(parse_status(b"garbage"), None);
        assert_eq!(parse_status(b"HTTP/1.1 abc\r\n"), None);
    }

    #[test]
    fn exit_code_follows_the_status() {
        let ok = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        assert!(check(ok, Duration::from_secs(2)).is_ok());
        let bad = serve_once(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n");
        assert!(check(bad, Duration::from_secs(2)).unwrap_err().contains("503"));
    }

    #[test]
    fn refused_connection_and_silence_fail() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = l.local_addr().unwrap();
        drop(l);
        assert!(check(closed, Duration::from_secs(2)).is_err());
        let silent = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = silent.local_addr().unwrap();
        let start = Instant::now();
        let e = check(addr, Duration::from_millis(300)).unwrap_err();
        assert!(e.contains("no answer"), "{e}");
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(silent);
    }
}
