use std::error::Error;

use crate::{
    db::{AlbumInfo, ArtistInfo, TrackInfo},
    deezer::{self, Artist, Deezer, PaginatedResponse},
    loading::Loading,
};
use chrono::{TimeDelta, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;

#[derive(Debug, Error, Clone)]
enum CacheUpdateError {
    #[error("deezer API error")]
    ApiError(#[source] deezer::ApiErrCode),
    #[error("internal error")]
    InternalError,
}

#[derive(Debug, Error)]
pub enum RetrievalError {
    #[error("internal cache update error")]
    CacheUpdateInternalError,
    #[error("deezer API error")]
    ApiError(#[from] deezer::ApiErrCode),
    #[error("database error")]
    DbError(#[from] sqlx::Error),
}

impl From<CacheUpdateError> for RetrievalError {
    fn from(err: CacheUpdateError) -> Self {
        use CacheUpdateError::*;
        match err {
            ApiError(err) => RetrievalError::ApiError(err),
            InternalError => RetrievalError::CacheUpdateInternalError,
        }
    }
}

fn to_internal_error<E: Error + 'static>(err: E) -> CacheUpdateError {
    let mut errstrings = Vec::new();
    let mut cerr: &(dyn Error + 'static) = &err;
    loop {
        errstrings.push(cerr.to_string());

        match cerr.source() {
            Some(e) => {
                cerr = e;
            }
            None => break,
        }
    }
    log::error!("cache update error: {}", errstrings.join(":"));
    CacheUpdateError::InternalError
}

impl From<deezer::Error> for CacheUpdateError {
    fn from(value: deezer::Error) -> Self {
        use deezer::Error;
        match value {
            Error::ApiError(e) => CacheUpdateError::ApiError(e),
            e => to_internal_error(e),
        }
    }
}

/// Represents the internal state of the quiz.
pub struct QuizState {
    loading: Loading<u32, Result<(ArtistInfo, Option<Vec<TrackInfo>>), CacheUpdateError>>,
    pool: PgPool,
    cache_duration: chrono::Duration,
    deezer: Deezer,
}

impl QuizState {
    /// Createa a new quiz from `conf`.
    pub fn new(db_address: &str, cache_duration: TimeDelta) -> Result<Self, sqlx::Error> {
        Ok(Self {
            loading: Loading::new(),
            pool: PgPool::connect_lazy(db_address)?,
            cache_duration,
            deezer: Deezer::new(),
        })
    }

    async fn update_cache(
        deezer: Deezer,
        mut trans: Transaction<'_, Postgres>,
        artist_id: u32,
    ) -> Result<(ArtistInfo, Option<Vec<TrackInfo>>), CacheUpdateError> {
        let artist = deezer.artist(artist_id).await?;
        let ainfo = ArtistInfo::from(artist);
        ainfo.delete(&mut trans).await.map_err(to_internal_error)?;

        ainfo.insert(&mut trans).await.map_err(to_internal_error)?;

        let mut tracks = Vec::new();

        for album in deezer.artist_albums(artist_id, 0, 300).await?.data {
            let album_tracks = match deezer.album_tracks(album.id, 0, 300).await {
                Ok(t) => t.data,
                Err(e) => {
                    log::warn!(
                        "Error getting tracks for album {album_id}: {e}",
                        album_id = album.id
                    );
                    continue;
                }
            };

            AlbumInfo::from_album(album.clone(), artist_id)
                .insert(&mut trans)
                .await
                .map_err(to_internal_error)?;

            for track in album_tracks {
                let trackinfo = TrackInfo::from_deezer(track, album.clone());
                trackinfo
                    .insert(&mut trans)
                    .await
                    .map_err(to_internal_error)?;
                tracks.push(trackinfo);
            }
        }

        trans.commit().await.map_err(to_internal_error)?;

        Ok((ainfo, Some(tracks)))
    }
    async fn update_cache_if_needed(
        &self,
        artist: u32,
    ) -> Result<(ArtistInfo, Option<Vec<TrackInfo>>), CacheUpdateError> {
        let mut trans = self.pool.begin().await.map_err(to_internal_error)?;
        let artist_opt = ArtistInfo::get_from_id(&mut trans, artist)
            .await
            .map_err(to_internal_error)?
            .filter(|a| a.updated_at + self.cache_duration > Utc::now());

        match artist_opt {
            Some(artist) => Ok((artist, None)),
            None => {
                self.loading
                    .run(
                        artist,
                        QuizState::update_cache(self.deezer.clone(), trans, artist),
                    )
                    .await
            }
        }
    }

    /// Retrieves artist wtih id `artist`, caching it as needed.
    pub async fn get_artist(&self, artist: u32) -> Result<ArtistInfo, RetrievalError> {
        let (artist, _) = self.update_cache_if_needed(artist).await?;
        Ok(artist)
    }

    /// Retrieves tracks by artist with id `artist`, caching as needed.
    pub async fn get_artist_tracks(&self, artist: u32) -> Result<Vec<TrackInfo>, RetrievalError> {
        match self.update_cache_if_needed(artist).await? {
            (_, Some(tracks)) => Ok(tracks),
            (_, None) => {
                Ok(TrackInfo::from_artist_id(self.pool.acquire().await?.as_mut(), artist).await?)
            }
        }
    }

    /// Searches for artists with names matching the given query using the Deezer API
    pub async fn search_artists(
        &self,
        q: &str,
        index: u32,
        limit: u32,
    ) -> Result<PaginatedResponse<Artist>, deezer::Error> {
        self.deezer.search_artist(q, index, limit).await
    }
}

#[cfg(test)]
mod test {
    use serial_test::serial;
    use tokio::select;

    use super::*;

    async fn setup_state(pool: sqlx::Pool<Postgres>) -> QuizState {
        eprintln!("waiting for 5 seconds to clear Deezer ratelimit");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        eprintln!("wait complete");
        QuizState {
            loading: Loading::new(),
            pool,
            cache_duration: TimeDelta::try_minutes(10).unwrap(),
            deezer: Deezer::new(),
        }
    }

    #[sqlx::test]
    #[serial]
    async fn test_get_artists(pool: sqlx::Pool<Postgres>) {
        let state = setup_state(pool).await;
        let artist = state.get_artist(56563392).await.unwrap();

        assert_eq!(artist.title, "Mili");

        assert!(
            ArtistInfo::get_from_id(&mut state.pool.acquire().await.unwrap(), 56563392)
                .await
                .unwrap()
                .is_some()
        )
    }

    #[sqlx::test]
    #[serial]
    async fn test_get_tracks_artist_cached(pool: sqlx::Pool<Postgres>) {
        let state = setup_state(pool).await;
        let _artist = state.get_artist(56563392).await.unwrap();

        let tracks = select! {
            trackres = state.get_artist_tracks(56563392) => {
                trackres.unwrap()
            },
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                panic!("request timed out")
            }
        };
        assert!(tracks
            .into_iter()
            .any(|track| track.title == "Ga1ahad and Scientific Witchery"));
    }

    #[sqlx::test]
    #[serial]
    async fn test_get_tracks_artist_noncached(pool: sqlx::Pool<Postgres>) {
        let state = setup_state(pool).await;
        let tracks = state.get_artist_tracks(56563392).await.unwrap();

        assert!(tracks
            .into_iter()
            .any(|track| track.title == "Ga1ahad and Scientific Witchery"));
    }
}
