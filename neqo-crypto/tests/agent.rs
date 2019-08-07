#![deny(warnings)]

use neqo_crypto::*;

mod handshake;
use crate::handshake::*;
use test_fixture::{fixture_init, now};

#[test]
fn make_client() {
    fixture_init();
    let _c = Client::new("server").expect("should create client");
}

#[test]
fn make_server() {
    fixture_init();
    let _s = Server::new(&["key"]).expect("should create server");
}

#[test]
fn basic() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    println!("client {:p}", &client);
    let mut server = Server::new(&["key"]).expect("should create server");
    println!("server {:p}", &server);

    let bytes = client.handshake(now(), &[]).expect("send CH");
    assert!(bytes.len() > 0);
    assert_eq!(*client.state(), HandshakeState::InProgress);

    let bytes = server
        .handshake(now(), &bytes[..])
        .expect("read CH, send SH");
    assert!(bytes.len() > 0);
    assert_eq!(*server.state(), HandshakeState::InProgress);

    let bytes = client.handshake(now(), &bytes[..]).expect("send CF");
    assert_eq!(bytes.len(), 0);
    assert_eq!(*client.state(), HandshakeState::AuthenticationPending);

    client.authenticated();
    assert_eq!(*client.state(), HandshakeState::Authenticated);

    // Calling handshake() again indicates that we're happy with the cert.
    let bytes = client.handshake(now(), &[]).expect("send CF");
    assert!(bytes.len() > 0);
    assert!(client.state().connected());

    let client_info = client.info().expect("got info");
    assert_eq!(TLS_VERSION_1_3, client_info.version());
    assert_eq!(TLS_AES_128_GCM_SHA256, client_info.cipher_suite());

    let bytes = server.handshake(now(), &bytes[..]).expect("finish");
    assert_eq!(bytes.len(), 0);
    assert!(server.state().connected());

    let server_info = server.info().expect("got info");
    assert_eq!(TLS_VERSION_1_3, server_info.version());
    assert_eq!(TLS_AES_128_GCM_SHA256, server_info.cipher_suite());
}

#[test]
fn raw() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    println!("client {:?}", client);
    let mut server = Server::new(&["key"]).expect("should create server");
    println!("server {:?}", server);

    let client_records = client.handshake_raw(now(), None).expect("send CH");
    assert!(client_records.len() > 0);
    assert_eq!(*client.state(), HandshakeState::InProgress);

    let client_preinfo = client.preinfo().expect("get preinfo");
    assert_eq!(client_preinfo.version(), None);
    assert_eq!(client_preinfo.cipher_suite(), None);
    assert_eq!(client_preinfo.early_data(), false);
    assert_eq!(client_preinfo.early_data_cipher(), None);
    assert_eq!(client_preinfo.max_early_data(), 0);
    assert_eq!(client_preinfo.alpn(), None);

    let server_records =
        forward_records(now(), &mut server, client_records).expect("read CH, send SH");
    assert!(server_records.len() > 0);
    assert_eq!(*server.state(), HandshakeState::InProgress);

    let server_preinfo = server.preinfo().expect("get preinfo");
    assert_eq!(server_preinfo.version(), Some(TLS_VERSION_1_3));
    assert_eq!(server_preinfo.cipher_suite(), Some(TLS_AES_128_GCM_SHA256));
    assert_eq!(server_preinfo.early_data(), false);
    assert_eq!(server_preinfo.early_data_cipher(), None);
    assert_eq!(server_preinfo.max_early_data(), 0);
    assert_eq!(server_preinfo.alpn(), None);

    let client_records = forward_records(now(), &mut client, server_records).expect("send CF");
    assert_eq!(client_records.len(), 0);
    assert_eq!(*client.state(), HandshakeState::AuthenticationPending);

    client.authenticated();
    assert_eq!(*client.state(), HandshakeState::Authenticated);

    // Calling handshake() again indicates that we're happy with the cert.
    let client_records = client.handshake_raw(now(), None).expect("send CF");
    assert!(client_records.len() > 0);
    assert!(client.state().connected());

    let server_records = forward_records(now(), &mut server, client_records).expect("finish");
    assert_eq!(server_records.len(), 0);
    assert!(server.state().connected());

    // The client should have one certificate for the server.
    let mut certs = client.peer_certificate().unwrap();
    let cert_vec: Vec<&[u8]> = certs.collect();
    assert_eq!(1, cert_vec.len());

    // The server shouldn't have a client certificate.
    assert!(server.peer_certificate().is_none());
}

#[test]
fn chacha_client() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    let mut server = Server::new(&["key"]).expect("should create server");
    client
        .enable_ciphers(&[TLS_CHACHA20_POLY1305_SHA256])
        .expect("ciphers set");

    connect(&mut client, &mut server);

    assert_eq!(
        client.info().unwrap().cipher_suite(),
        TLS_CHACHA20_POLY1305_SHA256
    );
    assert_eq!(
        server.info().unwrap().cipher_suite(),
        TLS_CHACHA20_POLY1305_SHA256
    );
}

#[test]
fn p256_server() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    let mut server = Server::new(&["key"]).expect("should create server");
    server
        .set_groups(&[TLS_GRP_EC_SECP256R1])
        .expect("groups set");

    connect(&mut client, &mut server);

    assert_eq!(client.info().unwrap().key_exchange(), TLS_GRP_EC_SECP256R1);
    assert_eq!(server.info().unwrap().key_exchange(), TLS_GRP_EC_SECP256R1);
}

#[test]
fn alpn() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    client.set_alpn(&["alpn"]).expect("should set ALPN");
    let mut server = Server::new(&["key"]).expect("should create server");
    server.set_alpn(&["alpn"]).expect("should set ALPN");

    connect(&mut client, &mut server);

    let expected = Some(String::from("alpn"));
    assert_eq!(expected.as_ref(), client.info().unwrap().alpn());
    assert_eq!(expected.as_ref(), server.info().unwrap().alpn());
}

#[test]
fn alpn_multi() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    client
        .set_alpn(&["dummy", "alpn"])
        .expect("should set ALPN");
    let mut server = Server::new(&["key"]).expect("should create server");
    server
        .set_alpn(&["alpn", "other"])
        .expect("should set ALPN");

    connect(&mut client, &mut server);

    let expected = Some(String::from("alpn"));
    assert_eq!(expected.as_ref(), client.info().unwrap().alpn());
    assert_eq!(expected.as_ref(), server.info().unwrap().alpn());
}

#[test]
fn alpn_server_pref() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    client
        .set_alpn(&["dummy", "alpn"])
        .expect("should set ALPN");
    let mut server = Server::new(&["key"]).expect("should create server");
    server
        .set_alpn(&["alpn", "dummy"])
        .expect("should set ALPN");

    connect(&mut client, &mut server);

    let expected = Some(String::from("alpn"));
    assert_eq!(expected.as_ref(), client.info().unwrap().alpn());
    assert_eq!(expected.as_ref(), server.info().unwrap().alpn());
}

#[test]
fn alpn_no_protocol() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    client.set_alpn(&["a"]).expect("should set ALPN");
    let mut server = Server::new(&["key"]).expect("should create server");
    server.set_alpn(&["b"]).expect("should set ALPN");

    connect_fail(&mut client, &mut server);

    // TODO(mt) check the error code
}

#[test]
fn alpn_client_only() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    client.set_alpn(&["alpn"]).expect("should set ALPN");
    let mut server = Server::new(&["key"]).expect("should create server");

    connect(&mut client, &mut server);

    assert_eq!(None, client.info().unwrap().alpn());
    assert_eq!(None, server.info().unwrap().alpn());
}

#[test]
fn alpn_server_only() {
    fixture_init();
    let mut client = Client::new("server.example").expect("should create client");
    let mut server = Server::new(&["key"]).expect("should create server");
    server.set_alpn(&["alpn"]).expect("should set ALPN");

    connect(&mut client, &mut server);

    assert_eq!(None, client.info().unwrap().alpn());
    assert_eq!(None, server.info().unwrap().alpn());
}

#[test]
fn resume() {
    let (_, token) = resumption_setup(Resumption::WithoutZeroRtt);

    let mut client = Client::new("server.example").expect("should create second client");
    let mut server = Server::new(&["key"]).expect("should create second server");

    client
        .set_resumption_token(&token[..])
        .expect("should accept token");
    connect(&mut client, &mut server);

    assert!(client.info().unwrap().resumed());
    assert!(server.info().unwrap().resumed());
}

#[test]
fn zero_rtt() {
    let (anti_replay, token) = resumption_setup(Resumption::WithZeroRtt);

    // Finally, 0-RTT should succeed.
    let mut client = Client::new("server.example").expect("should create client");
    let mut server = Server::new(&["key"]).expect("should create server");
    client
        .set_resumption_token(&token[..])
        .expect("should accept token");
    client.enable_0rtt().expect("should enable 0-RTT");
    server
        .enable_0rtt(
            anti_replay.as_ref().unwrap(),
            0xffff_ffff,
            PermissiveZeroRttChecker::new(),
        )
        .expect("should enable 0-RTT");

    connect(&mut client, &mut server);
    assert!(client.info().unwrap().early_data_accepted());
    assert!(server.info().unwrap().early_data_accepted());
}

#[test]
fn zero_rtt_no_eoed() {
    let (anti_replay, token) = resumption_setup(Resumption::WithZeroRtt);

    // Finally, 0-RTT should succeed.
    let mut client = Client::new("server.example").expect("should create client");
    let mut server = Server::new(&["key"]).expect("should create server");
    client
        .set_resumption_token(&token[..])
        .expect("should accept token");
    client.enable_0rtt().expect("should enable 0-RTT");
    client.disable_end_of_early_data();
    server
        .enable_0rtt(
            anti_replay.as_ref().unwrap(),
            0xffff_ffff,
            PermissiveZeroRttChecker::new(),
        )
        .expect("should enable 0-RTT");
    server.disable_end_of_early_data();

    connect(&mut client, &mut server);
    assert!(client.info().unwrap().early_data_accepted());
    assert!(server.info().unwrap().early_data_accepted());
}

#[derive(Debug)]
struct RejectZeroRtt {}
impl ZeroRttChecker for RejectZeroRtt {
    fn check(&self, token: &[u8]) -> ZeroRttCheckResult {
        assert_eq!(ZERO_RTT_TOKEN_DATA, token);
        ZeroRttCheckResult::Reject
    }
}

#[test]
fn reject_zero_rtt() {
    let (anti_replay, token) = resumption_setup(Resumption::WithZeroRtt);

    // Finally, 0-RTT should succeed.
    let mut client = Client::new("server.example").expect("should create client");
    let mut server = Server::new(&["key"]).expect("should create server");
    client
        .set_resumption_token(&token[..])
        .expect("should accept token");
    client.enable_0rtt().expect("should enable 0-RTT");
    server
        .enable_0rtt(
            anti_replay.as_ref().unwrap(),
            0xffff_ffff,
            Box::new(RejectZeroRtt {}),
        )
        .expect("should enable 0-RTT");

    connect(&mut client, &mut server);
    assert!(!client.info().unwrap().early_data_accepted());
    assert!(!server.info().unwrap().early_data_accepted());
}
