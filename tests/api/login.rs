use crate::helpers;
use crate::helpers::{assert_is_redirect_to, spawn_app};

/// Cookies are set by attaching a special HTTP header to the response-`Set-Cookie`. In its simplest
/// form it looks like this:
/// `Set-Cookie: {cookie-name}={cookie-value}`
///
/// `Set-Cookie` can be specified multiple times - one for each cookie you want to set. reqwest
/// provides the `get-all` method to deal with multi-value headers:
///
/// When it comes to durability, there are two types of cookies: **session cookies** and **persistent
/// cookies**. Session cookies are stored in memory - they are deleted when the session ends (i.e.
/// the browser is closed). Persistent cookies, instead, are saved to disk and will still be there
/// when you re-open the browser.
///
/// A vanilla `Set-Cookie` header creates a session cookie. To set a persistent cookie you must
/// specify an expiration policy using a cookie attribute - either `Max-Age` or `Expires`. `Max-Age`
/// is interpreted as the number of seconds remaining until the cookie expires. `Expires` instead,
/// expects a data.
///
/// Setting `Max-Age` to 0 instructs the browser to immediately expire the cookie - i.e. to unset it.
#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    // Arrange
    let app = spawn_app().await;

    // Act - Part 1 - Try to login
    let login_body = serde_json::json!({
        "username": "random-username",
        "password": "random-password"
    });
    let response = app.post_login(&login_body).await;

    // Assert
    assert_eq!(response.status().as_u16(), 303);
    helpers::assert_is_redirect_to(&response, "/login");

    // let flash_cookie = response.cookies().find(|c| c.name() == "_flash").unwrap();
    // assert_eq!(flash_cookie.value(), "Authentication failed");

    // Act - Part2 - Follow the redirect
    let html_page = app.get_login_html().await;
    assert!(html_page.contains(r#"<p><i>Authentication failed</i></p>"#));

    // Act - Part3 - Reload the login page
    let html_page = app.get_login_html().await;
    assert!(!html_page.contains(r#"<p><i>Authentication failed</i></p>"#));
}

#[tokio::test]
async fn redirect_to_admin_dashboard_after_login_success() {
    // Arrange
    let app = spawn_app().await;

    // Act - Part1 - Login
    let login_body = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/admin/dashboard");

    // Act - Part 2 - Follow the redirect
    let html_page = app.get_admin_dashboard_html().await;
    assert!(html_page.contains(&format!("Welcome {}", app.test_user.username)));
}
