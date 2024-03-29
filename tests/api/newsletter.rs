use crate::helpers::{assert_is_redirect_to, spawn_app, ConfirmationLinks, TestApp};
use fake::faker::internet::en::SafeEmail;
use fake::faker::name::en::Name;
use fake::Fake;
use std::time::Duration;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;
    create_unconfirmed_subscriber(&app).await;
    app.login().await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        // We assert that no request is fired at Postmark!
        .expect(0)
        .mount(&app.email_server)
        .await;

    // Act - Part 1 - Submit newsletter form
    // A sketch of the newsletter payload structure.
    // serde_urlencoded cannot decode nested json objects, so keep all the fields at top level for now.
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content" : "Newsletter body as plain text",
        "html_content" : "<p>Newsletter body as HTML</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string()
    });

    // For some reason, the post_publish_newsletter function is not able to serialize json
    // value into a string.
    let response = app.post_publish_newsletter(&newsletter_request_body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 2 - Follow the redirect
    let html_page = app.get_publish_newsletter_html().await;

    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - emails will go out shortly.</i></p>"
    ));
    app.dispatch_all_pending_emails().await;

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
async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    // We are working with multiple subscribers now, their details must be randomized to avoid conflicts!
    let name: String = Name().fake();
    let email: String = SafeEmail().fake();
    //let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let body = serde_urlencoded::to_string(&serde_json::json!({
        "name": name,
        "email": email
    }))
    .unwrap();

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

    // We now inspect the requests received by the mock Postmark server to retrieve the confirmation
    // link and return it.
    let email_request = &app
        .email_server
        .received_requests()
        .await
        .unwrap()
        .pop()
        .unwrap();

    app.get_confirmation_links(&email_request)
}

async fn create_confirmed_subscriber(app: &TestApp) {
    // We can then reuse the same helper and just add an extra step to actually call the confirmation
    // link!
    let confirmation_link = create_unconfirmed_subscriber(app).await;

    reqwest::get(confirmation_link.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

#[ignore]
#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;
    app.login().await;

    Mock::given(method("POST"))
        .and(path("/email"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    dbg!(&app.email_server.uri());
    // Act - Part 1 - Submit newsletter form
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content" : "Newsletter body as plain text",
        "html_content" : "<p>Newsletter body as HTML</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string()
    });
    let response = app.post_publish_newsletter(&newsletter_request_body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 2 - Follow the redirect
    let html_page = app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - emails will go out shortly.</i></p>"
    ));
    app.dispatch_all_pending_emails().await;

    // Mock verifies on Drop that we have sent the newsletter email
}

/// # Basic Authentication
/// The API must look for the `Authorization` header in the incoming request, structured as follows:
///
/// `Authorization: Basic <encoded credentials>`
///
/// where `<encoded credentials>` is the base64-encoding of {username}:{password}.
///
/// base64-encoding ensures that all the characters in the output are ASCII, but it does not provide
/// any kind of protection: decoding requires no secrets. In other words, encoding is not encryption.
#[tokio::test]
async fn you_must_be_logged_in_to_publish_a_newsletter() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string()
    });

    let response = app.post_publish_newsletter(&newsletter_request_body).await;

    // Assert
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_be_logged_in_to_see_the_newsletter_form() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get_publish_newsletter().await;

    // Assert
    assert_is_redirect_to(&response, "/login");
}

#[ignore]
#[tokio::test]
async fn newsletter_creation_is_idempotent() {
    // Arrange
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;
    app.login().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act - Part 1 - Submit Newsletter form
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        // We expect idempotency key as part of the form data, not as a header
        "idempotency_key": uuid::Uuid::new_v4().to_string()
    });
    let response = app.post_publish_newsletter(&newsletter_request_body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 2 - Follow the redirect
    let html_page = app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - emails will go out shortly.</i></p>"
    ));

    // Act - part 3 - Submit newsletter form **again**
    let response = app.post_publish_newsletter(&newsletter_request_body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - part 4 - Follow the redirect
    let html_page = app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - emails will go out shortly.</i></p>"
    ));

    app.dispatch_all_pending_emails().await;

    // Mock verifies on Drop that we have sent the newsletter email **once**
}

#[ignore]
#[tokio::test]
async fn concurrent_form_submission_is_handled_gracefully() {
    // Arrange
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;
    app.login().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        // Setting a long delay to ensure that the second request
        // arrives before the first one completes
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act - Submit two newsletter forms concurrently
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string()
    });
    let response1 = app.post_publish_newsletter(&newsletter_request_body);
    let response2 = app.post_publish_newsletter(&newsletter_request_body);

    let (response1, response2) = tokio::join!(response1, response2);

    assert_eq!(response1.status(), response2.status());
    assert_eq!(
        response1.text().await.unwrap(),
        response2.text().await.unwrap()
    );

    app.dispatch_all_pending_emails().await;

    // Mock verifies on Drop that we have sent the newsletter email **once**
}
