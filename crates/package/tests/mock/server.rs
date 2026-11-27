use super::auth::{self, Config, Frame};
use simplex_package::Credentials;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Record {
    pub method: String,
    pub target: String,
    pub headers: BTreeMap<String, String>,
}

struct State {
    config: Config,
    objects: BTreeMap<String, Vec<u8>>,
    records: Vec<Record>,
}

pub struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
    handle: Option<JoinHandle<()>>,
}

impl Server {
    pub fn start(
        bucket: &str,
        region: &str,
        credentials: Option<Credentials>,
        limit: Option<usize>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock s3");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let address = listener.local_addr().expect("listener addr");
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(State {
            config: Config {
                bucket: bucket.to_owned(),
                region: region.to_owned(),
                credentials,
                limit,
            },
            objects: BTreeMap::new(),
            records: Vec::new(),
        }));
        let flag = Arc::clone(&stop);
        let shared = Arc::clone(&state);
        let handle = thread::spawn(move || accept(listener, flag, shared));
        Self {
            url: format!("http://{address}"),
            stop,
            state,
            handle: Some(handle),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn insert(&self, key: &str, bytes: Vec<u8>) {
        let mut guard = self.state.lock().expect("lock state");
        guard.objects.insert(key.to_owned(), bytes);
    }

    pub fn keys(&self) -> Vec<String> {
        let guard = self.state.lock().expect("lock state");
        guard.objects.keys().cloned().collect()
    }

    pub fn records(&self) -> Vec<Record> {
        let guard = self.state.lock().expect("lock state");
        guard.records.clone()
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept(listener: TcpListener, stop: Arc<AtomicBool>, state: Arc<Mutex<State>>) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => serve(stream, &state),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(_) => break,
        }
    }
}

fn serve(mut stream: TcpStream, state: &Arc<Mutex<State>>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(frame) = parse(&mut stream) else {
        return;
    };
    let (status, payload) = dispatch(state, &frame);
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        412 => "Precondition Failed",
        _ => "Bad Request",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&payload);
    let _ = stream.flush();
}

fn parse(stream: &mut TcpStream) -> Result<Frame, ()> {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    if reader.read_line(&mut first).map_err(|_| ())? == 0 {
        return Err(());
    }
    let parts: Vec<&str> = first.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(());
    }
    let headers = headers(&mut reader)?;
    let length = headers
        .get("content-length")
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).map_err(|_| ())?;
    Ok(Frame {
        method: parts[0].to_owned(),
        target: parts[1].to_owned(),
        headers,
        body,
    })
}

fn headers(reader: &mut BufReader<&mut TcpStream>) -> Result<BTreeMap<String, String>, ()> {
    let mut map = BTreeMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|_| ())?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            return Ok(map);
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            map.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
}

fn dispatch(state: &Arc<Mutex<State>>, frame: &Frame) -> (u16, Vec<u8>) {
    let mut guard = state.lock().expect("lock state");
    guard.records.push(Record {
        method: frame.method.clone(),
        target: frame.target.clone(),
        headers: frame.headers.clone(),
    });
    if !auth::authorize(&guard.config, frame) {
        return (403, b"access denied".to_vec());
    }
    let (path, query) = frame.target.split_once('?').unwrap_or((&frame.target, ""));
    let prefix = format!("/{}", guard.config.bucket);
    if path == prefix && frame.method == "GET" {
        return auth::listing(&guard.objects, guard.config.limit, query);
    }
    let Some(key) = path
        .strip_prefix(&format!("{prefix}/"))
        .filter(|rest| !rest.is_empty())
    else {
        return (404, Vec::new());
    };
    object(&mut guard, key, frame)
}

fn object(guard: &mut State, key: &str, frame: &Frame) -> (u16, Vec<u8>) {
    match frame.method.as_str() {
        "PUT" => {
            if frame.headers.get("if-none-match").map(String::as_str) != Some("*") {
                return (400, b"missing precondition".to_vec());
            }
            if guard.objects.contains_key(key) {
                return (412, b"immutable".to_vec());
            }
            guard.objects.insert(key.to_owned(), frame.body.clone());
            (200, Vec::new())
        }
        "GET" => match guard.objects.get(key) {
            Some(bytes) => (200, bytes.clone()),
            None => (404, Vec::new()),
        },
        _ => (400, Vec::new()),
    }
}
