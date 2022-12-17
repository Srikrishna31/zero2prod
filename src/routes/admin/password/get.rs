use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use tera::{Context, Tera};

pub async fn change_password_form(
    templates: web::Data<&Tera>,
) -> Result<HttpResponse, actix_web::Error> {
    let context = Context::new();
    Ok(HttpResponse::Ok().content_type(ContentType::html()).body(
        templates
            .render("change_password_form.html", &context)
            .unwrap(),
    ))
}
