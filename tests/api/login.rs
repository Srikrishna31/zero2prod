use crate::helpers;
use crate::helpers::spawn_app;

/// Cookies are set by attaching a special HTTP header to the response-`Set-Cookie`. In its simplest
/// form it looks like this:
/// `Set-Cookie: {cookie-name}={cookie-value}`
///
/// `Set-Cookie` can be specified multiple times - one for each cookie you want to set. reqwest
/// provides the `get-all` method to deal with multi-value headers:
#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let login_body = serde_json::json!({
        "username": "random-username",
        "password": "random-password"
    });
    let response = app.post_login(&login_body).await;

    // Assert
    assert_eq!(response.status().as_u16(), 303);
    helpers::assert_is_redirect_to(&response, "/login");

    let flash_cookie = response.cookies().find(|c| c.name() == "_flash").unwrap();
    assert_eq!(flash_cookie.value(), "Authentication failed");
}
