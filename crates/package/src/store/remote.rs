use crate::Fault;
use crate::signing::{self, Endpoint, Request};
use crate::store::clock;
use std::io::Read;
use ureq::Agent;

pub(crate) struct Client<'a> {
    endpoint: &'a Endpoint,
}

impl<'a> Client<'a> {
    pub(crate) fn new(endpoint: &'a Endpoint) -> Self {
        Self { endpoint }
    }

    pub(crate) fn put(&self, key: &str, payload: &[u8]) -> Result<(), Fault> {
        let timestamp = clock::now();
        let request = Request {
            method: "PUT",
            path: key,
            query: "",
            payload,
        };
        let headers = signing::sign(self.endpoint, request, &timestamp, Some("*"))?;
        let (url, loopback) = signing::target(self.endpoint, key, "")?;
        let client = agent(loopback);
        let mut call = client.put(&url);
        for (name, value) in &headers {
            call = call.header(name, value);
        }
        let response = call
            .send(payload)
            .map_err(|err| Fault::Remote(format!("put {key} failed: {err}")))?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        if matches!(status, 409 | 412) {
            return Err(Fault::Immutable(format!("object already exists: {key}")));
        }
        Err(Fault::Remote(format!(
            "put {key} failed with status {status}"
        )))
    }

    pub(crate) fn get(&self, key: &str) -> Result<Vec<u8>, Fault> {
        let timestamp = clock::now();
        let request = Request {
            method: "GET",
            path: key,
            query: "",
            payload: b"",
        };
        let headers = signing::sign(self.endpoint, request, &timestamp, None)?;
        let (url, loopback) = signing::target(self.endpoint, key, "")?;
        let client = agent(loopback);
        let mut call = client.get(&url);
        for (name, value) in &headers {
            call = call.header(name, value);
        }
        let mut response = call
            .call()
            .map_err(|err| Fault::Remote(format!("get {key} failed: {err}")))?;
        let status = response.status().as_u16();
        if status == 404 {
            return Err(Fault::Missing(format!("missing remote object: {key}")));
        }
        if !(200..300).contains(&status) {
            return Err(Fault::Remote(format!(
                "get {key} failed with status {status}"
            )));
        }
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut body)
            .map_err(|err| Fault::Remote(format!("read {key} failed: {err}")))?;
        Ok(body)
    }

    pub(crate) fn list(&self, prefix: &str) -> Result<Vec<String>, Fault> {
        let mut keys = Vec::new();
        let mut continuation: Option<String> = None;
        for _ in 0..1024 {
            let query = match &continuation {
                None => format!("list-type=2&prefix={prefix}"),
                Some(token) => format!("continuation-token={token}&list-type=2&prefix={prefix}"),
            };
            let xml = self.page(&query)?;
            extract(&xml, "Key", &mut keys);
            if !truncated(&xml) {
                return Ok(keys);
            }
            let next = tag(&xml, "NextContinuationToken").map(unescape);
            if next.is_none() || next == continuation {
                return Ok(keys);
            }
            continuation = next;
        }
        Ok(keys)
    }

    fn page(&self, query: &str) -> Result<String, Fault> {
        let timestamp = clock::now();
        let request = Request {
            method: "GET",
            path: "",
            query,
            payload: b"",
        };
        let headers = signing::sign(self.endpoint, request, &timestamp, None)?;
        let (url, loopback) = signing::target(self.endpoint, "", query)?;
        let client = agent(loopback);
        let mut call = client.get(&url);
        for (name, value) in &headers {
            call = call.header(name, value);
        }
        let mut response = call
            .call()
            .map_err(|err| Fault::Remote(format!("list failed: {err}")))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(Fault::Remote(format!("list failed with status {status}")));
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut bytes)
            .map_err(|err| Fault::Remote(format!("read list body failed: {err}")))?;
        String::from_utf8(bytes).map_err(|err| Fault::Remote(format!("invalid list xml: {err}")))
    }
}

fn agent(loopback: bool) -> Agent {
    let mut builder = Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0);
    if loopback {
        builder = builder.proxy(None);
    }
    builder.build().into()
}

fn extract(xml: &str, element: &str, out: &mut Vec<String>) {
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        out.push(unescape(&after[..end]));
        rest = &after[end + close.len()..];
    }
}

fn tag<'a>(xml: &'a str, element: &str) -> Option<&'a str> {
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    let start = xml.find(&open)?;
    let after = &xml[start + open.len()..];
    let end = after.find(&close)?;
    Some(&after[..end])
}

fn truncated(xml: &str) -> bool {
    matches!(tag(xml, "IsTruncated").map(str::trim), Some("true"))
}

fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
