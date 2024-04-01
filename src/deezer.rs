use std::iter;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use reqwest::Request;
use reqwest::Response;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_repr::Deserialize_repr;
use thiserror::Error;
use tokio::task::JoinSet;
use tokio::time::Instant;
use url::Url;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use lazy_static::lazy_static;

lazy_static! {
    static ref DEEZER_API_BASE: Url = Url::parse("https://api.deezer.com").unwrap();
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("request error")]
    ReqwestError(#[from] reqwest::Error),
    #[error("url parse error")]
    UrlParseError(#[from] url::ParseError),
    #[error("api error")]
    ApiError(#[from] ApiErrCode)
}

/// Represents an artist as returned from the Deezer API.
#[derive(Clone, Debug, Deserialize)]
pub struct Artist {
    pub id: u32,
    pub name: String,
    pub link: Url,
    pub picture: Url,
    pub picture_small: Url,
    pub picture_medium: Url,
    pub picture_big: Url,
    pub picture_xl: Url,
    pub nb_album: u32,
    pub radio: bool,
    pub tracklist: Url,
}

/// Represents an album as returned from the Deezer API.
#[derive(Clone, Debug, Deserialize)]
pub struct Album {
    pub id: u32,
    pub title: String,
    pub link: Url,
    pub cover: Url,
    pub cover_small: Url,
    pub cover_medium: Url,
    pub cover_big: Url,
    pub cover_xl: Url,
    pub release_date: NaiveDate,
}

/// Represents a track as returned from the Deezer API.
#[derive(Clone, Debug, Deserialize)]
pub struct Track {
    pub id: u32,
    pub readable: bool,
    pub title: String,
    pub title_short: String,
    pub link: Url,
    pub duration: u32,
    pub rank: u32,
    pub explicit_lyrics: bool,
    pub preview: Url,
}

/// Represents a paginated response as returned from the Deezer API.
#[derive(Clone, Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: u32,
}


struct DeezerRequest {
    req: Request,
    tx: oneshot::Sender<Result<reqwest::Response, reqwest::Error>>,
}

/// Represents the Deezer client. Requests are properly ratelimited.
#[derive(Clone)]
pub struct Deezer {
    tx: mpsc::Sender<DeezerRequest>,
}

async fn serve_deezer(mut rx: mpsc::Receiver<DeezerRequest>) {
    let c = Arc::new(reqwest::Client::new());
    loop {
        let t = Instant::now();
        let mut set = JoinSet::new();

        for _ in 0..45 {
            let Some(r) = rx.recv().await else { return };
            let cref = Arc::clone(&c);
            set.spawn(async move {
                r.tx.send(cref.execute(r.req).await).expect("oneshot channel closed");
            });
        }

        while set.join_next().await.is_some() {}
        tokio::time::sleep_until(t + Duration::from_secs(5)).await;
    }
}

#[derive(Clone, Deserialize_repr, Error, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ApiErrCode {
    #[error("Quota reached")]
    Quota = 4,
    #[error("Item limit exceeded")]
    ItemsLimitExceeded = 100,
    #[error("Invalid permissions")]
    Permission = 200,
    #[error("Invalid token")]
    TokenInvalid = 300,
    #[error("Invalid parameter")]
    Parameter = 500,
    #[error("Missing parameter")]
    ParameterMissing = 501,
    #[error("Invalid query")]
    QueryInvalid = 600,
    #[error("Service busy")]
    ServiceBusy = 700,
    #[error("Not found")]
    DataNotFound = 800,
    #[error("Individual account not found")]
    IndividualAccountNotAllowed = 901,
    #[error("Unknown error")]
    #[serde(other)]
    Other = 0
}

#[derive(Deserialize)]
struct DeezerResponseError {
    code: ApiErrCode
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DeezerResponse<T> {
    Error { error: DeezerResponseError },
    Ok(T),
}

impl Deezer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(45);
        tokio::spawn(serve_deezer(rx));
        Self { tx }
    }

    async fn send_rq(&self, req: Request) -> Result<Response, Error> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(DeezerRequest { req, tx }).await.unwrap();
        rx.await.expect("oneshot channel closed").map_err(Into::into)
    }

    async fn get<'a, T, PathIt, It, K, V>(&self, path: PathIt, params: It) -> Result<T, Error>
    where T: DeserializeOwned,
          PathIt: IntoIterator<Item = &'a str>,
          It: IntoIterator<Item = (K, V)>,
          K: AsRef<str>,
          V: AsRef<str>
    {
        let mut url = DEEZER_API_BASE.clone();
        url.path_segments_mut().unwrap().extend(path);
        url.query_pairs_mut().extend_pairs(params);
        let rq = Request::new(Method::GET, url);
        let retval: DeezerResponse<T> = self.send_rq(rq)
            .await?
            .error_for_status()?
            .json()
            .await?;

        match retval {
            DeezerResponse::Error { error: DeezerResponseError { code } } =>
                Err(Error::ApiError(code)),
            DeezerResponse::Ok(val) =>
                Ok(val)
        }
    }

    /// Searches artists.
    pub async fn search_artist(&self, q: &str, index: u32, limit: u32) -> Result<PaginatedResponse<Artist>, Error> {
        self.get(["search", "artist"],
                 [("q", q),
                  ("index", &index.to_string()),
                  ("limit", &limit.to_string())]).await
    }

    /// Gets artist by ID
    pub async fn artist(&self, id: u32) -> Result<Artist, Error> {
        self.get(["artist", &id.to_string()],
                 iter::empty::<(&str, &str)>()).await
    }

    /// Gets albums by artist
    pub async fn artist_albums(&self, id: u32, index: u32, limit: u32) -> Result<PaginatedResponse<Album>, Error> {
        self.get(["artist", &id.to_string(), "albums"],
                 [("index", &index.to_string()),
                  ("limit", &limit.to_string())]).await
    }

    /// Gets tracks in album
    pub async fn album_tracks(&self, id: u32, index: u32, limit: u32) -> Result<PaginatedResponse<Track>, Error> {
        self.get(["album", &id.to_string(), "tracks"],
                 [("index", &index.to_string()),
                  ("limit", &limit.to_string())]).await
    }
}

impl Default for Deezer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_search_artist() -> Result<(), Error> {
        let deez = Deezer::new();
        let artists = deez.search_artist("Mili", 0, 25).await?;
        assert_eq!(artists.data[0].name, "Mili");
        Ok(())
    }

    #[tokio::test]
    async fn test_artist() -> Result<(), Error> {
        let deez = Deezer::new();
        let artist = deez.artist(56563392).await?;
        assert_eq!(artist.name, "Mili");
        Ok(())
    }

    #[tokio::test]
    async fn test_missing_artist() {
        let deez = Deezer::new();
        let artist_err = match deez.artist(909409309).await.unwrap_err() {
            Error::ApiError(e) => e,
            other => panic!("expected reqwest error, got: {other:#?}"),
        };

        assert_eq!(artist_err, ApiErrCode::DataNotFound);
    }

    #[tokio::test]
    async fn test_artist_albums() -> Result<(), Error> {
        let deez = Deezer::new();
        let albums = deez.artist_albums(56563392, 0, 100).await?;
        assert!(albums.data.into_iter().any(|a| a.title == "Millennium Mother"));
        Ok(())
    }

    #[tokio::test]
    async fn test_album_tracks() -> Result<(), Error> {
        let deez = Deezer::new();
        let tracks = deez.album_tracks(59795132, 0, 100).await?;
        assert!(tracks.data.into_iter().any(|a| a.title == "Summoning 101"));
        Ok(())
    }
}
