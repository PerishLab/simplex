use crate::protocol::{self, Credentials, Packet, Reply};
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub method: String,
    pub key: String,
    pub guard: Option<String>,
    pub signed: bool,
    pub status: u16,
}

struct Store {
    bucket: String,
    region: String,
    identity: String,
    secret: String,
    token: Option<String>,
    public: bool,
    stopped: bool,
    objects: BTreeMap<String, Vec<u8>>,
    events: Vec<Event>,
}

pub struct Server {
    endpoint: String,
    bucket: String,
    address: SocketAddr,
    state: Arc<Mutex<Store>>,
    worker: Option<JoinHandle<()>>,
}

impl Server {
    pub fn start(bucket: &str, region: &str, public: bool) -> Self {
        Self::launch(bucket, region, public, None)
    }

    pub fn tokenized(bucket: &str, region: &str, token: &str) -> Self {
        Self::launch(bucket, region, false, Some(token.to_owned()))
    }

    fn launch(bucket: &str, region: &str, public: bool, token: Option<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock s3");
        let address = listener.local_addr().expect("local addr");
        let state = Arc::new(Mutex::new(Store {
            bucket: bucket.to_owned(),
            region: region.to_owned(),
            identity: String::from("simplexkey"),
            secret: String::from("simplexsecret"),
            token,
            public,
            stopped: false,
            objects: BTreeMap::new(),
            events: Vec::new(),
        }));
        let shared = Arc::clone(&state);
        let worker = thread::spawn(move || serve(listener, shared));
        Self {
            endpoint: format!("http://{address}"),
            bucket: bucket.to_owned(),
            address,
            state,
            worker: Some(worker),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn seed(&self, key: &str, bytes: Vec<u8>) {
        let mut guard = self.state.lock().expect("lock store");
        guard.objects.insert(key.to_owned(), bytes);
    }

    pub fn object(&self, key: &str) -> Option<Vec<u8>> {
        let guard = self.state.lock().expect("lock store");
        guard.objects.get(key).cloned()
    }

    pub fn keys(&self) -> Vec<String> {
        let guard = self.state.lock().expect("lock store");
        guard.objects.keys().cloned().collect()
    }

    pub fn events(&self) -> Vec<Event> {
        let guard = self.state.lock().expect("lock store");
        guard.events.clone()
    }

    pub fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.state.lock().expect("lock store").stopped = true;
            let _ = TcpStream::connect(self.address);
            worker.join().expect("join server");
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

fn serve(listener: TcpListener, state: Arc<Mutex<Store>>) {
    for incoming in listener.incoming() {
        if state.lock().expect("lock store").stopped {
            break;
        }
        if let Ok(mut stream) = incoming {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            if let Some(packet) = protocol::receive(&mut stream) {
                let head = packet.method == "HEAD";
                let reply = handle(&state, packet);
                protocol::send(&mut stream, reply, head);
            }
        }
    }
}

fn handle(state: &Arc<Mutex<Store>>, packet: Packet) -> Reply {
    let mut store = state.lock().expect("lock store");
    let key = extract(&packet.path, &store.bucket);
    let label = key.clone().unwrap_or_default();
    let guard = packet.headers.get("if-none-match").cloned();
    let signed = match authorize(&store, &packet) {
        Ok(value) => value,
        Err(reason) => {
            record(&mut store, &packet.method, &label, (guard, false, 403));
            return error(403, "Forbidden", &reason);
        }
    };
    let Some(object) = key else {
        record(&mut store, &packet.method, &label, (guard, signed, 404));
        return error(404, "Not Found", "NoSuchBucket");
    };
    let reply = dispatch(&mut store, &packet, &object, guard.as_deref());
    record(
        &mut store,
        &packet.method,
        &object,
        (guard, signed, reply.status),
    );
    reply
}

fn authorize(store: &Store, packet: &Packet) -> Result<bool, String> {
    if packet.headers.contains_key("authorization") {
        let credentials = Credentials {
            identity: &store.identity,
            secret: &store.secret,
            region: &store.region,
            token: store.token.as_deref(),
        };
        protocol::verify(&credentials, packet)?;
        return Ok(true);
    }
    if store.public && (packet.method == "GET" || packet.method == "HEAD") {
        return Ok(false);
    }
    Err(String::from("AccessDenied"))
}

fn extract(path: &str, bucket: &str) -> Option<String> {
    let trimmed = path.strip_prefix('/')?;
    if trimmed == bucket {
        return Some(String::new());
    }
    let prefix = format!("{bucket}/");
    trimmed.strip_prefix(&prefix).map(str::to_owned)
}

fn dispatch(store: &mut Store, packet: &Packet, object: &str, guard: Option<&str>) -> Reply {
    if packet.method == "GET" && object.is_empty() && listing(&packet.query) {
        return catalog(store, &prefix(&packet.query));
    }
    if packet.method == "PUT" {
        if object.is_empty() || guard != Some("*") {
            return error(400, "Bad Request", "MissingPrecondition");
        }
        if store.objects.contains_key(object) {
            return error(412, "Precondition Failed", "PreconditionFailed");
        }
        store
            .objects
            .insert(object.to_owned(), packet.payload.clone());
        return Reply {
            status: 200,
            reason: "OK",
            kind: "application/octet-stream",
            body: Vec::new(),
        };
    }
    if packet.method == "GET" || packet.method == "HEAD" {
        if let Some(bytes) = store.objects.get(object) {
            return Reply {
                status: 200,
                reason: "OK",
                kind: "application/octet-stream",
                body: bytes.clone(),
            };
        }
        return error(404, "Not Found", "NoSuchKey");
    }
    error(405, "Method Not Allowed", "MethodNotAllowed")
}

fn listing(query: &str) -> bool {
    query.split('&').any(|part| part == "list-type=2")
}

fn prefix(query: &str) -> String {
    for part in query.split('&') {
        if let Some(value) = part.strip_prefix("prefix=") {
            return protocol::decode(value);
        }
    }
    String::new()
}

fn catalog(store: &Store, filter: &str) -> Reply {
    let matched: Vec<(&String, &Vec<u8>)> = store
        .objects
        .iter()
        .filter(|(key, _)| key.starts_with(filter))
        .collect();
    let mut document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>{}</Name><Prefix>{filter}</Prefix><KeyCount>{}</KeyCount><MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated>",
        store.bucket,
        matched.len()
    );
    for (key, bytes) in matched {
        document.push_str(&format!(
            "<Contents><Key>{key}</Key><Size>{}</Size></Contents>",
            bytes.len()
        ));
    }
    document.push_str("</ListBucketResult>");
    Reply {
        status: 200,
        reason: "OK",
        kind: "application/xml",
        body: document.into_bytes(),
    }
}

fn record(store: &mut Store, method: &str, key: &str, meta: (Option<String>, bool, u16)) {
    store.events.push(Event {
        method: method.to_owned(),
        key: key.to_owned(),
        guard: meta.0,
        signed: meta.1,
        status: meta.2,
    });
}

fn error(status: u16, reason: &'static str, code: &str) -> Reply {
    Reply {
        status,
        reason,
        kind: "application/xml",
        body: format!("<Error><Code>{code}</Code></Error>").into_bytes(),
    }
}
