use crate::helpers::{spawn_app, TestApp};
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;
    create_unconfirmed_subscriber(&app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        // We assert that no request is fired at Postmark!
        .expect(0)
        .mount(&app.email_server)
        .await;

    // Act

    // A sketch of the newsletter payload structure.
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "content" : {
            "text" : "Newsletter body as plain text",
            "html" : "<p>Newsletter body as HTML</p>",
        }
    });
    let response = reqwest::Client::new()
        .post(&format!("{}/newsletters", &app.address))
        .json(&newsletter_request_body)
        .send()
        .await
        .expect("Failed to execute request.");

    // Assert
    assert_eq!(response.status().as_u16(), 200);
    //Mock verifies on Drop that we haven't sent the newsletter email
}

/// Use the public API of the application under test to create an unconfirmed subscriber.
/// # Scoped Mocks
/// With `mount`, the behavior we specify remains active as long as the underlying `MockServer` is up
/// and running.
///
/// With `mount_as_scoped`, instead we get back a guard object - a `MockGuard`.
///
/// `MockGuard` has a custom `Drop` implementation: when it goes out of scope, `wiremock` instructs
/// the underlying `MockServer` to stop honouring the specified mock behavior. In other words, we stop
/// returning 200 to `POST /email` at the end of `create_unconfirmed_subscriber`. The mock behavior
/// needed for our test helper **stays local** to the test helper itself.
///
/// One more thing happens when a `MockGuard` is dropped - we **eagerly** check that expectations on
/// the scoped mock are verified. This creates a useful feedback loop to keep our test helpers clean
/// and up-to-date.
async fn create_unconfirmed_subscriber(app: &TestApp) {
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    let _mock_guard = Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;

    app.post_subscriptions(body.into())
        .await
        .error_for_status()
        .unwrap();
}
