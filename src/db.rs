use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::deezer::{Album, Artist, Track};

/// Represents info about an artist, corresponding with the `artists` table in the database
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ArtistInfo {
    pub id: i64,
    pub title: String,
    pub icon_url: String,
    pub updated_at: DateTime<Utc>,
}

impl ArtistInfo {
    /// Gets an `ArtistInfo` corresponding to `id` from the database
    pub async fn get_from_id(
        conn: &mut PgConnection,
        id: u32,
    ) -> Result<Option<ArtistInfo>, sqlx::Error> {
        sqlx::query_as!(
            ArtistInfo,
            "select id, title, icon_url, updated_at from artists where id = $1",
            i64::from(id)
        )
        .fetch_optional(conn)
        .await
    }

    /// Inserts `self` into the database
    pub async fn insert(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "insert into artists (id, title, icon_url, updated_at) values ($1, $2, $3, $4)",
            self.id,
            self.title,
            self.icon_url,
            self.updated_at
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    /// Deletes `self` from the database
    pub async fn delete(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        sqlx::query!("delete from artists where id = $1", self.id)
            .execute(conn)
            .await?;
        Ok(())
    }
}

impl From<Artist> for ArtistInfo {
    fn from(a: Artist) -> Self {
        Self {
            id: a.id.into(),
            title: a.name,
            icon_url: a.picture_medium.to_string(),
            updated_at: Utc::now(),
        }
    }
}

/// Represents info about an album, corresponding with the `albums` table in the database.
#[derive(Debug, Clone)]
pub struct AlbumInfo {
    pub id: i64,
    pub artist: i64,
    pub title: String,
    pub cover_url: String,
}

impl AlbumInfo {
    /// Creates an `AlbumInfo` from an [`Album`] with the given `artist_id`.
    pub fn from_album(value: Album, artist_id: u32) -> Self {
        Self {
            id: value.id.into(),
            artist: artist_id.into(),
            title: value.title,
            cover_url: value.cover_medium.to_string(),
        }
    }

    /// Inserts `self` into the database.
    pub async fn insert(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "insert into albums (id, artist, title, cover_url) values ($1, $2, $3, $4)",
            self.id,
            self.artist,
            self.title,
            self.cover_url
        )
        .execute(conn)
        .await?;
        Ok(())
    }
}

/// Represents info about a track. This corresponds with the `tracks` table, partially joined with `albums`.
#[derive(Debug, Clone, Serialize)]
pub struct TrackInfo {
    pub id: i64,
    pub album_title: String,
    pub album_cover_url: String,
    pub album_id: i64,
    pub title: String,
    pub preview_url: String,
}

impl TrackInfo {
    /// Creates a `TrackInfo` from a [`Track`] and an [`Album`]
    pub fn from_deezer(track: Track, album: Album) -> Self {
        Self {
            id: track.id.into(),
            album_title: album.title,
            album_cover_url: album.cover_medium.to_string(),
            album_id: album.id.into(),
            title: track.title,
            preview_url: track.preview.to_string(),
        }
    }

    /// Inserts `self` into the database.
    pub async fn insert(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "insert into tracks (id, album, title, preview_url) values ($1, $2, $3, $4)",
            self.id,
            self.album_id,
            self.title,
            self.preview_url
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    /// Retrieves all tracks from the artist with id `artist_id` from the database.
    pub async fn from_artist_id(
        conn: &mut PgConnection,
        artist_id: u32,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            TrackInfo,
            "select
                                        tracks.id as id,
                                        albums.title as album_title,
                                        albums.cover_url as album_cover_url,
                                        albums.id as album_id,
                                        tracks.title as title,
                                        tracks.preview_url as preview_url
                                    from
                                        albums join tracks ON albums.id = tracks.album
                                    where
                                        albums.artist = $1",
            i64::from(artist_id)
        )
        .fetch_all(conn)
        .await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::Duration;
    use sqlx::{pool::PoolConnection, Postgres};

    #[sqlx::test]
    async fn test_artistinfo(mut conn: PoolConnection<Postgres>) -> Result<(), sqlx::Error> {
        let artist_id = 56563392;
        let artist = ArtistInfo {
            id: artist_id.into(),
            title: "Mili".to_owned(),
            icon_url: "https://very-nice-website.com/icon.png".to_owned(),
            updated_at: Utc::now(),
        };

        artist.insert(&mut conn).await?;

        let retrived_artist = ArtistInfo::get_from_id(&mut conn, artist_id)
            .await?
            .expect("no artist inserted");
        assert_eq!(retrived_artist.id, artist.id);
        assert_eq!(retrived_artist.title, artist.title);
        assert_eq!(retrived_artist.icon_url, artist.icon_url);
        assert!(
            (retrived_artist.updated_at - artist.updated_at).abs()
                < Duration::try_seconds(1).unwrap()
        );
        Ok(())
    }
}
