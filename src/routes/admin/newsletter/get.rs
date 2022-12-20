use actix_web::http::header::ContentType;
use actix_web::{HttpResponse, web};
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;
use tera::Tera;

pub async fn publish_newsletter_form(
    flash_messages: IncomingFlashMessages,
    templates: web::Data<&Tera>,
) -> Result<HttpResponse, actix_web::Error> {
    let mut msg_html = String::new();
    for m in flash_messages.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html()))
}
