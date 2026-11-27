mod archive;
mod audit;
mod consumer;
mod fixture;
mod harness;
mod lemma;
mod mock;
mod targets;
mod workspace;

use mock::{crypto, protocol, server};

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;

#[test]
fn primitives() {
    assert_eq!(
        crypto::render(&crypto::digest(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        crypto::render(&crypto::digest(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        protocol::canonicalize("prefix=perish/codec/&list-type=2"),
        "list-type=2&prefix=perish%2Fcodec%2F"
    );

    let mut headers = BTreeMap::new();
    headers.insert(String::from("host"), String::from("127.0.0.1:9000"));
    headers.insert(String::from("if-none-match"), String::from("*"));
    let content = crypto::render(&crypto::digest(b"hello"));
    headers.insert(String::from("x-amz-content-sha256"), content.clone());
    headers.insert(String::from("x-amz-date"), String::from("20261127T120000Z"));
    let canonical = format!(
        "PUT\n/simplex/perish/wire/1.0.0/stable/source.tar\n\nhost:127.0.0.1:9000\nif-none-match:*\nx-amz-content-sha256:{content}\nx-amz-date:20261127T120000Z\n\nhost;if-none-match;x-amz-content-sha256;x-amz-date\n{content}"
    );
    let hashed = crypto::render(&crypto::digest(canonical.as_bytes()));
    let message =
        format!("AWS4-HMAC-SHA256\n20261127T120000Z\n20261127/auto/s3/aws4_request\n{hashed}");
    let signing = crypto::derive("simplexsecret", "20261127", "auto");
    let signature = crypto::render(&crypto::authenticate(&signing, message.as_bytes()));
    headers.insert(
        String::from("authorization"),
        format!(
            "AWS4-HMAC-SHA256 Credential=simplexkey/20261127/auto/s3/aws4_request, SignedHeaders=host;if-none-match;x-amz-content-sha256;x-amz-date, Signature={signature}"
        ),
    );
    let credentials = protocol::Credentials {
        identity: "simplexkey",
        secret: "simplexsecret",
        region: "auto",
        token: None,
    };
    let packet = protocol::Packet {
        method: String::from("PUT"),
        path: String::from("/simplex/perish/wire/1.0.0/stable/source.tar"),
        query: String::new(),
        headers,
        payload: b"hello".to_vec(),
    };
    assert!(protocol::verify(&credentials, &packet).is_ok());

    let mut mock = server::Server::start("public", "auto", true);
    mock.seed("perish/wire/1.0.0/stable/seal.toml", b"sealed".to_vec());
    let address = mock
        .endpoint()
        .strip_prefix("http://")
        .expect("http url")
        .to_owned();
    let mut stream = TcpStream::connect(&address).expect("connect mock");
    let request = format!(
        "GET /public/perish/wire/1.0.0/stable/seal.toml HTTP/1.1\r\nHost: {address}\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).expect("write get");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.ends_with("sealed"));
    mock.stop();
}
