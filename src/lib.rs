use chrono::TimeDelta;
use serde::de::Error;
use serde::{de::Unexpected, Deserialize, Deserializer};

pub mod db;
pub mod deezer;
pub mod loading;
pub mod routing;
pub mod state;

const WEBSITE_NAME: &str = "quiz.make.id.lv";

#[derive(Deserialize)]
pub struct Config {
    pub database_url: String,
    #[serde(deserialize_with = "deser_timedelta")]
    pub cache_duration: TimeDelta,
    pub bind_address: String,
}

/// Parses a timedelta in the format "1d 2h 3m 2s".
pub fn parse_timedelta(s: &str) -> Option<TimeDelta> {
    s.split_whitespace()
        .map(|x| {
            let (idx, c) = x.char_indices().next_back()?;
            let num: i64 = x[..idx].parse().ok()?;
            let comp = match c {
                's' => TimeDelta::try_seconds(num)?,
                'm' => TimeDelta::try_minutes(num)?,
                'h' => TimeDelta::try_hours(num)?,
                'd' => TimeDelta::try_days(num)?,
                _ => return None,
            };
            Some(comp)
        })
        .try_fold(TimeDelta::zero(), |a, b| Some(a + b?))
}

/// Uses [`parse_timedelta`] to deserialize a [`TimeDelta`]
pub fn deser_timedelta<'de, D>(deserializer: D) -> Result<TimeDelta, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    parse_timedelta(&s).ok_or_else(|| D::Error::invalid_value(Unexpected::Str(&s), &"a duration"))
}
