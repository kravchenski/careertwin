//! The link checker against a local server: what is reachable, what is gone, what redirects, what hangs.

use std::net::TcpListener;
use std::time::Duration;

use careertwin_tools::links::{self, Fields, check_url, client};
use serde_json::{Value, json};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

const BOTH: Fields = Fields {
    careers: true,
    domain: true,
};

async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/ok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(path("/gone"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(path("/moved"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/ok"))
        .mount(&server)
        .await;
    Mock::given(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    server
}

/// An address nothing listens on.
fn closed_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
async fn a_page_that_answers_is_ok_and_a_missing_one_is_broken() {
    let server = server().await;
    let client = client(Duration::from_secs(5)).unwrap();
    let ok = check_url(&client, &format!("{}/ok", server.uri())).await;
    assert_eq!(
        (ok.status, ok.ok, ok.final_url.is_none(), ok.error),
        (Some(200), true, true, None)
    );
    let gone = check_url(&client, &format!("{}/gone", server.uri())).await;
    assert_eq!((gone.status, gone.ok), (Some(404), false));
    assert_eq!(gone.detail(), "HTTP 404");
}

#[tokio::test]
async fn a_redirect_is_followed_and_says_where_it_ended() {
    let server = server().await;
    let client = client(Duration::from_secs(5)).unwrap();
    let moved = check_url(&client, &format!("{}/moved", server.uri())).await;
    assert_eq!((moved.status, moved.ok), (Some(200), true));
    assert_eq!(moved.final_url, Some(format!("{}/ok", server.uri())));
}

#[tokio::test]
async fn a_bare_address_is_not_reported_as_redirected() {
    // the address is written without a path and comes back with one: that is the same page
    let server = MockServer::start().await;
    Mock::given(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let check = check_url(&client(Duration::from_secs(5)).unwrap(), &server.uri()).await;
    assert!(check.ok && check.final_url.is_none(), "{check:?}");
}

#[tokio::test]
async fn a_server_that_hangs_is_a_timeout_and_one_that_is_not_there_is_a_connect_error() {
    let server = server().await;
    let client = client(Duration::from_millis(400)).unwrap();
    let slow = check_url(&client, &format!("{}/slow", server.uri())).await;
    assert_eq!(
        (slow.ok, slow.status, slow.error.as_deref()),
        (false, None, Some("timeout"))
    );
    let nobody = check_url(&client, &format!("http://127.0.0.1:{}/", closed_port())).await;
    assert_eq!(
        (nobody.ok, nobody.error.as_deref()),
        (false, Some("ConnectError"))
    );
    assert_eq!(nobody.detail(), "ConnectError");
}

fn companies(server: &MockServer) -> Vec<Value> {
    vec![
        json!({"name": "Fine", "careers": format!("{}/ok", server.uri())}),
        json!({"name": "Gone", "careers": format!("{}/gone", server.uri()), "domain": format!("127.0.0.1:{}", closed_port())}),
        json!({"name": "No page"}),
    ]
}

#[tokio::test]
async fn the_results_follow_the_list_and_only_the_asked_fields_are_checked() {
    let server = server().await;
    let list = companies(&server);
    let careers = links::run(
        &list,
        Fields::parse("careers").unwrap(),
        2,
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(
        careers
            .iter()
            .map(|entry| entry.name.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["Fine", "Gone", "No page"]
    );
    assert!(
        careers.iter().all(|entry| entry.website.is_none()),
        "the website is not asked for"
    );
    assert!(
        careers[0].careers.as_ref().unwrap().ok
            && !careers[1].careers.as_ref().unwrap().ok
            && careers[2].careers.is_none()
    );

    let both = links::run(&list, BOTH, 2, Duration::from_secs(5))
        .await
        .unwrap();
    let website = both[1].website.as_ref().unwrap();
    assert_eq!(
        (website.ok, website.url.starts_with("https://127.0.0.1:")),
        (false, true)
    );
}

#[tokio::test]
async fn the_report_lists_what_is_broken_and_says_so_when_nothing_is() {
    let server = server().await;
    let list = companies(&server);
    let results = links::run(
        &list,
        Fields::parse("careers").unwrap(),
        4,
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    let report = links::report(&results);
    assert!(
        report.starts_with("Checked 3 companies.\n\n1 broken link(s):\n\n"),
        "{report}"
    );
    assert!(
        report.contains(&format!(
            "  - Gone [careers]: {}/gone  ->  HTTP 404\n",
            server.uri()
        )),
        "{report}"
    );
    assert_eq!(links::broken(&results).len(), 1);

    let fine = links::run(
        &list[..1],
        Fields::parse("careers").unwrap(),
        4,
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(
        links::report(&fine),
        "Checked 1 companies.\nAll links OK.\n"
    );
    assert!(links::broken(&fine).is_empty());
}

#[tokio::test]
async fn the_json_report_keeps_the_names_of_the_fields() {
    let server = server().await;
    let list = companies(&server);
    let results = links::run(&list[..2], BOTH, 4, Duration::from_secs(5))
        .await
        .unwrap();
    let json = results[1].to_json();
    assert_eq!(json["name"], "Gone");
    assert_eq!(json["careers"]["status"], 404);
    assert_eq!(json["careers"]["ok"], false);
    assert_eq!(json["careers"]["final_url"], Value::Null);
    assert_eq!(json["website"]["error"], "ConnectError");
    let keys: Vec<&String> = json.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["name", "careers", "website"]);
    assert!(
        results[0].to_json().get("website").is_none(),
        "a company without a domain has no website entry"
    );
}
