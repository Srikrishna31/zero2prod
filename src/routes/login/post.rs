use actix_web::http::header::LOCATION;
use actix_web::HttpResponse;

/// # Redirect on Success
/// A redirect response requires two elements:
/// * a redirect status code;
/// * a `Location` header, set to the URL we want to redirect to.
///
/// All redirect status codes are in the 3xx range -  we need to choose the most appropriate one
/// depending on the HTTP verb and the semantic meaning we want to communicate(e.g. temporary vs
/// permanent redirection).
pub async fn login() -> HttpResponse {
    HttpResponse::SeeOther()
        //Go back to the home page.
        .insert_header((LOCATION, "/"))
        .finish()
}
