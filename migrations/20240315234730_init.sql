-- Add migration script here
CREATE TABLE artists (
    id BIGINT PRIMARY KEY,
    title TEXT NOT NULL,
    icon_url TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE albums (
    id BIGINT PRIMARY KEY,
    artist BIGINT NOT NULL REFERENCES artists(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    cover_url TEXT NOT NULL
);

CREATE TABLE tracks (
    id BIGINT PRIMARY KEY,
    album BIGINT NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    preview_url TEXT NOT NULL
);
