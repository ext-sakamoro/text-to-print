//! What `GalleryClient::publish` / `delete` send is accepted by the Worker's check
//!
//! A mock endpoint records each request body; the body's signature must verify,
//! with the same code the Worker uses (`text_to_print_gallery_sign`), over the
//! message of the fields the body carries, and the fields must be the ones the
//! caller passed

use ed25519_dalek::SigningKey;
use text_to_print_network::gallery_client::GalleryClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ID: &str = "018f4e8c-0000-7000-8000-000000000001";

fn identity() -> (SigningKey, [u8; 32], String) {
    let key = SigningKey::from_bytes(&[11u8; 32]);
    let public = key.verifying_key().to_bytes();
    (key, public, format!("did:key:{}", hex::encode(public)))
}

fn signature(body: &serde_json::Value) -> [u8; 64] {
    let mut out = [0u8; 64];
    hex::decode_to_slice(body["signature"].as_str().unwrap(), &mut out).unwrap();
    out
}

async fn only_body(server: &MockServer) -> serde_json::Value {
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    serde_json::from_slice(&requests[0].body).unwrap()
}

#[tokio::test]
async fn a_published_post_verifies_over_the_fields_it_carries() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/publish"))
        .respond_with(ResponseTemplate::new(202))
        .mount(&server)
        .await;
    let (key, public, did) = identity();
    let client = GalleryClient::new(server.uri());
    client
        .publish(
            &key,
            ID,
            &did,
            Some("maker"),
            "cube(10)",
            "2026-10-24T00:00:00Z",
        )
        .await
        .unwrap();

    let body = only_body(&server).await;
    assert_eq!(body["id"], ID);
    assert_eq!(body["author_did"], did.as_str());
    assert_eq!(body["author_nickname"], "maker");
    assert_eq!(body["lol_source"], "cube(10)");
    assert_eq!(body["created_at"], "2026-10-24T00:00:00Z");
    let message =
        text_to_print_gallery_sign::publish_message(&text_to_print_gallery_sign::PublishFields {
            id: ID,
            author_did: &did,
            author_nickname: Some("maker"),
            lol_source: "cube(10)",
            created_at: "2026-10-24T00:00:00Z",
        })
        .unwrap();
    assert!(text_to_print_gallery_sign::verify(
        public,
        &message,
        signature(&body)
    ));
}

#[tokio::test]
async fn a_delete_verifies_over_its_post_and_author() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/{ID}")))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let (key, public, did) = identity();
    let client = GalleryClient::new(server.uri());
    client.delete(&key, ID, &did).await.unwrap();

    let body = only_body(&server).await;
    assert_eq!(body["author_did"], did.as_str());
    let message = text_to_print_gallery_sign::delete_message(ID, &did).unwrap();
    assert!(text_to_print_gallery_sign::verify(
        public,
        &message,
        signature(&body)
    ));
}
