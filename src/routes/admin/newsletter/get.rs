use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;
use tera::{Context, Tera};

pub async fn publish_newsletter_form(
    flash_messages: IncomingFlashMessages,
    templates: web::Data<&Tera>,
) -> Result<HttpResponse, actix_web::Error> {
    let mut msg_html = String::new();
    for m in flash_messages.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }

    let mut context = Context::new();
    context.insert("msg_html", &msg_html);

    let html_body = templates.render("newsletter_form.html", &context).unwrap();
    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(html_body))
}
