use axum::{
    extract::{Form, Path, Query},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::markdown::render_markdown;
use crate::notes::{self, Note};
use crate::pages;
use crate::themes::get_theme;

#[derive(Clone)]
pub struct AppState {
    // Could hold shared config in the future
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/notes", post(create_note_route))
        .route(
            "/notes/:id",
            get(show_note_route).post(update_or_delete_note_route),
        )
        .route("/preview", post(preview_route))
        .route("/theme", post(theme_route))
        .route("/about", get(about_route))
        .route("/export", get(export_route))
        .with_state(Arc::new(AppState {}))
}

fn get_theme_from_cookie(headers: &HeaderMap) -> String {
    headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            if key == "theme" {
                Some(value.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Dark".to_string())
}

#[derive(Deserialize)]
struct IndexQuery {
    q: Option<String>,
}

async fn index(
    Query(query): Query<IndexQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let theme = get_theme_from_cookie(&headers);
    let all_notes = notes::list_notes()?;

    let notes: Vec<Note> = if let Some(ref q) = query.q {
        let q = q.to_lowercase();
        all_notes
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&q) || n.content.to_lowercase().contains(&q)
            })
            .collect()
    } else {
        all_notes
    };

    let html = pages::render_app(&theme, &notes, None, query.q.as_deref(), None);
    Ok(Html(html.into_string()))
}

async fn show_note_route(
    Path(id): Path<String>,
    Query(query): Query<IndexQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let theme = get_theme_from_cookie(&headers);
    let note = notes::load_note(&id)?;
    let all_notes = notes::list_notes()?;

    let notes: Vec<Note> = if let Some(ref q) = query.q {
        let q = q.to_lowercase();
        all_notes
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&q) || n.content.to_lowercase().contains(&q)
            })
            .collect()
    } else {
        all_notes
    };

    let html = pages::render_app(&theme, &notes, Some(&note), query.q.as_deref(), None);
    Ok(Html(html.into_string()))
}

#[derive(Deserialize)]
struct NoteForm {
    title: Option<String>,
    content: Option<String>,
    #[serde(default)]
    _method: String,
}

async fn create_note_route(headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    let _theme = get_theme_from_cookie(&headers);
    let note = notes::create_note("Untitled", "")?;
    Ok(Redirect::to(&format!("/notes/{}", note.id)))
}

async fn update_or_delete_note_route(
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<NoteForm>,
) -> Result<impl IntoResponse, AppError> {
    let _theme = get_theme_from_cookie(&headers);

    match form._method.as_str() {
        "delete" => {
            notes::delete_note(&id)?;
            Ok(Redirect::to("/").into_response())
        }
        _ => {
            let title = form.title.unwrap_or_else(|| "Untitled".to_string());
            let content = form.content.unwrap_or_default();
            notes::update_note(&id, title, content)?;
            Ok(Redirect::to(&format!("/notes/{}", id)).into_response())
        }
    }
}

#[derive(Deserialize)]
struct PreviewForm {
    content: String,
}

async fn preview_route(Form(form): Form<PreviewForm>) -> Result<impl IntoResponse, AppError> {
    let html = render_markdown(&form.content);
    Ok(Html(pages::render_preview_fragment(&html).into_string()))
}

#[derive(Deserialize)]
struct ThemeForm {
    theme: String,
}

async fn theme_route(
    _headers: HeaderMap,
    Form(form): Form<ThemeForm>,
) -> Result<impl IntoResponse, AppError> {
    // Validate theme name
    let _ = get_theme(&form.theme);

    let mut response = Redirect::to("/").into_response();
    let cookie = format!(
        "theme={}; Path=/; Max-Age=31536000; SameSite=Lax",
        form.theme
    );
    response
        .headers_mut()
        .insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());
    Ok(response)
}

async fn about_route(headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    let theme = get_theme_from_cookie(&headers);
    Ok(Html(pages::render_about(&theme).into_string()))
}

async fn export_route(_headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    let all_notes = notes::list_notes()?;
    let mut export = String::from("# Rasuti Notes Export\n\n");
    for note in all_notes {
        export.push_str(&format!("## {}\n\n{}\n\n---\n\n", note.title, note.content));
    }

    let mut response = Html(export).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        "attachment; filename=\"rasuti-notes-export.md\""
            .parse()
            .unwrap(),
    );
    Ok(response)
}

#[derive(Debug)]
pub struct AppError(anyhow::Error);

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let not_found = self
            .0
            .root_cause()
            .downcast_ref::<std::io::Error>()
            .map(|e| e.kind() == std::io::ErrorKind::NotFound)
            .unwrap_or(false);

        if not_found {
            return (
                axum::http::StatusCode::NOT_FOUND,
                "Note not found".to_string(),
            )
                .into_response();
        }

        let body = format!("Internal Server Error: {}", self.0);
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}
