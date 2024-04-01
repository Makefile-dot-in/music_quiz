use std::fmt::Debug;
use std::time::Duration;

use actix_web::http::header::CacheDirective;
use actix_web::middleware::{DefaultHeaders, Logger, NormalizePath, TrailingSlash};
use actix_web::web::{PathConfig, QueryConfig};
use actix_web::{App, HttpServer, Responder};
use actix_web::{body::BoxBody, get, web, HttpResponse, ResponseError};
use actix_web::http::{header, StatusCode};
use actix_web_lab::middleware::CatchPanic;
use actix_files as fs;
use askama::Template;
use askama_actix::TemplateToResponse;
use chrono::Utc;
use crate::db::TrackInfo;
use crate::deezer::Artist;
use crate::Config;
use crate::{db::ArtistInfo, deezer, state::{QuizState, RetrievalError}};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::error::Error;
use rand::prelude::*;
use tokio::select;




#[derive(Debug, Error)]
enum QuizError {
    #[error("deezer error")]
    Deezer(#[from] deezer::Error), // 19
    #[error("database error")]
    DbError(#[from] sqlx::Error),
    #[error("unkown internal error")]
    UnknownError,
    #[error("timeout")]
    Timeout,
}

#[derive(Template)]
#[template(path = "errors/internal.html")]
struct InternalErrorView;

#[derive(Template)]
#[template(path = "errors/loading.html")]
struct LoadingErrorView;

#[derive(Debug, Error, Template)]
#[template(path = "errors/invalidurl.html")]
struct InvalidReqView<E> {
    #[from] err: E
}

#[derive(Template)]
#[template(path = "errors/urlnotfound.html")]
struct UrlNotFoundView;

#[derive(Template)]
#[template(path = "errors/notfound.html")]
struct NotFoundView;

impl From<RetrievalError> for QuizError {
    fn from(value: RetrievalError) -> Self {
        match value {
            RetrievalError::CacheUpdateInternalError => Self::UnknownError,
            RetrievalError::ApiError(err) => Self::Deezer(deezer::Error::ApiError(err)),
            RetrievalError::DbError(err) => Self::DbError(err)
        }
    }
}

impl<E: Debug + Error> ResponseError for InvalidReqView<E> {
    fn error_response(&self) -> HttpResponse<BoxBody> {
        let mut resp = self.to_response();
        *resp.status_mut() = self.status_code();
        resp
    }

    fn status_code(&self) -> StatusCode { StatusCode::BAD_REQUEST }
}

impl ResponseError for QuizError {
    fn error_response(&self) -> HttpResponse<BoxBody> {
        let mut resp = match self {
            Self::Deezer(deezer::Error::ApiError(deezer::ApiErrCode::DataNotFound)) =>
                NotFoundView.to_response(),
            Self::Timeout =>
                LoadingErrorView.to_response(),
            _ =>
                InternalErrorView.to_response()
        };
        *resp.status_mut() = self.status_code();
        resp
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::Deezer(deezer::Error::ApiError(deezer::ApiErrCode::DataNotFound)) => StatusCode::NOT_FOUND,
            Self::Timeout => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[derive(Template)]
#[template(path = "artist.html", escape = "html")]
struct ArtistPageView {
    artist: ArtistInfo,
}

#[get("/artist/{id}")]
async fn artist_page(state: web::Data<QuizState>, id: web::Path<u32>) -> Result<impl Responder, QuizError> {
    let artist = select! {
        artist = state.get_artist(*id) => {
            artist?
        }
        _ = tokio::time::sleep(Duration::from_secs(5)) => {
            return Err(QuizError::Timeout)
        }
    };


    let updated_at = artist.updated_at;
    let resp = ArtistPageView { artist }.customize()
        .insert_header((header::AGE, (updated_at - Utc::now()).num_seconds()));
    Ok(resp)
}


#[derive(Serialize)]
struct Question {
    answer_info: TrackInfo,
    options: Vec<String>
}

#[get("/artist/{id}/questions.json")]
async fn artist_questions(state: web::Data<QuizState>, id: web::Path<u32>) -> Result<impl Responder, QuizError> {
    let mut tracks = state.get_artist_tracks(*id).await?;
    let mut rng = thread_rng();

    // this filters out duplicates, keeping random entries each time to add variety
    tracks.sort_unstable_by(|a, b| a.title.cmp(&b.title));

    for chunk in tracks.chunk_by_mut(|a, b| a.title == b.title) {
        chunk.shuffle(&mut rng);
    }

    tracks.dedup_by(|a, b| a.title == b.title);

    // finally, we shuffle all the tracks
    tracks.shuffle(&mut rng);

    let questions: Vec<_> = tracks
        .iter()
        .map(|track| {
            let mut options: Vec<_> = tracks
                .choose_multiple(&mut rng, 4) // we choose 4 so we can discard one if it is a duplicate
                .filter(|qtr| qtr.id != track.id) // filter out the current track in case it happened to be chosen
                .map(|qtr| qtr.title.clone())
                .take(3)
                .collect();

            options.push(track.title.clone());
            options.shuffle(&mut rng); // reshuffle to ensure the random placement of the correct answer

            Question {
                answer_info: track.clone(),
                options
            }
        })
        .collect();
    Ok(web::Json(questions))
}

#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
}

#[derive(Template)]
#[template(path = "search.html", escape="html")]
struct SearchView {
    results: Vec<Artist>,
}

#[get("/")]
async fn search(state: web::Data<QuizState>, query: web::Query<SearchParams>) -> Result<SearchView, QuizError> {
    let results = match &query.q {
        Some(q) if q != "" => state.search_artists(q, 0, 10).await?.data,
        _ => Vec::new(),
    };
    Ok(SearchView { results })
}


#[derive(Debug, Error)]
pub enum QuizInitError {
    #[error("database error")]
    DbError(#[from] sqlx::Error),
    #[error("IO error")]
    IoError(#[from] std::io::Error),
    #[error("config parsing error")]
    ConfigError(#[from] toml::de::Error)
}


pub async fn start_server(c: Config) -> Result<(), QuizInitError> {
    let Config { database_url, cache_duration, bind_address } = c;

    let data = web::Data::new(QuizState::new(&database_url, cache_duration)?);

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .service(fs::Files::new("/static", "static"))
            .service(artist_page)
            .service({
                web::scope("")
                    .service(artist_questions)
                    .wrap(DefaultHeaders::default()
                          .add(header::CacheControl(vec![
                              CacheDirective::NoCache
                          ])))
            })
            .service(search)
            .app_data(PathConfig::default().error_handler(|err, _| InvalidReqView { err }.into()))
            .app_data(QueryConfig::default().error_handler(|err, _| InvalidReqView { err }.into()))
            .default_service(web::to(|| async { (UrlNotFoundView, StatusCode::NOT_FOUND) }))
            .wrap({
                DefaultHeaders::default()
                    .add(header::CacheControl(vec![
                        CacheDirective::MaxAge(cache_duration.num_seconds().try_into().unwrap_or(u32::MAX))
                    ]))
            })
            .wrap(NormalizePath::new(TrailingSlash::Trim))
            .wrap(CatchPanic::default())
            .wrap(Logger::default())
    })
        .bind(bind_address)?
        .run()
        .await?;

    Ok(())
}
