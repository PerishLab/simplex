use simplex_package::signing::{Request, sign};
use simplex_package::{Credentials, Endpoint, Fault};

fn sample() -> Credentials {
    Credentials {
        identity: "AKIAIOSFODNN7EXAMPLE".into(),
        secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
        token: None,
    }
}

#[test]
fn vectors() {
    let endpoint = Endpoint {
        url: "https://s3.amazonaws.com".into(),
        bucket: "examplebucket".into(),
        region: "us-east-1".into(),
        credentials: Some(sample()),
    };
    let get = Request {
        method: "GET",
        path: "perish/codec/1.0.0/stable/seal.toml",
        query: "",
        payload: b"",
    };
    let headers = sign(&endpoint, get, "20130524T000000Z", None).expect("sign get");
    let prefixed = Request {
        path: "/examplebucket/perish/codec/1.0.0/stable/seal.toml",
        ..get
    };
    assert_eq!(
        sign(&endpoint, prefixed, "20130524T000000Z", None).expect("prefixed"),
        headers
    );
    assert_eq!(headers["host"], "s3.amazonaws.com");
    assert_eq!(
        headers["authorization"],
        "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=11ca1ab3ed26bdcf4aafc67ee8011e40c501847a6a217a9f8ba967b753505ccb"
    );

    let list = Request {
        method: "GET",
        path: "",
        query: "prefix=perish/codec/&list-type=2",
        payload: b"",
    };
    let sorted = sign(&endpoint, list, "20130524T000000Z", None).expect("sign list");
    assert_eq!(
        sorted["authorization"],
        "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=84ed6eb800ec975a51dcdbe90cbaea256c9399c8d7795650009921a63229b899"
    );

    let session = Endpoint {
        url: "http://127.0.0.1:9000".into(),
        bucket: "examplebucket".into(),
        region: "us-east-1".into(),
        credentials: Some(Credentials {
            token: Some("IQoJb3JpZ2luX2Vj".into()),
            ..sample()
        }),
    };
    let put = Request {
        method: "PUT",
        path: "perish/codec/1.0.0/stable/source.tar",
        query: "",
        payload: b"hello world",
    };
    let write = sign(&session, put, "20130524T000000Z", Some("*")).expect("sign put");
    assert_eq!(write["if-none-match"], "*");
    assert_eq!(write["x-amz-security-token"], "IQoJb3JpZ2luX2Vj");
    assert_eq!(
        write["authorization"],
        "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;if-none-match;x-amz-content-sha256;x-amz-date;x-amz-security-token, Signature=727d0cf12d81222624ee44a3220ba4267908c08cc053048c83b73572558a9550"
    );

    let oversized = Endpoint {
        credentials: Some(Credentials {
            identity: "AKIAIOSFODNN7EXAMPLE".into(),
            secret: "k".repeat(96),
            token: None,
        }),
        ..endpoint.clone()
    };
    let long = sign(&oversized, get, "20130524T000000Z", None).expect("long secret");
    assert!(long["authorization"].starts_with("AWS4-HMAC-SHA256 "));
    assert!(matches!(
        sign(&endpoint, get, "2013-05-24", None),
        Err(Fault::Remote(_))
    ));
}

#[test]
fn unsigned() {
    let public = Endpoint {
        url: "https://cdn.example.com:443".into(),
        bucket: "packages".into(),
        region: "auto".into(),
        credentials: None,
    };
    let read = Request {
        method: "GET",
        path: "perish/codec/1.0.0/stable/seal.toml",
        query: "",
        payload: b"",
    };
    let headers = sign(&public, read, "", None).expect("unsigned get");
    assert_eq!(
        headers.get("host").map(String::as_str),
        Some("cdn.example.com")
    );
    assert!(!headers.contains_key("authorization"));

    let write = Request {
        method: "PUT",
        path: "perish/codec/1.0.0/stable/source.tar",
        query: "",
        payload: b"tar",
    };
    assert!(matches!(
        sign(&public, write, "20261127T131548Z", Some("*")),
        Err(Fault::Remote(_))
    ));
}
