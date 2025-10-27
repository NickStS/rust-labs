use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;
use url::Url;
use chrono::{DateTime, Utc};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardUrl(Url);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TariffId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money(u64);


impl<'de> Deserialize<'de> for UserId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let id = Uuid::parse_str(&s).map_err(serde::de::Error::custom)?;
        Ok(UserId(id))
    }
}
impl<'de> Deserialize<'de> for ShardUrl {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let url = Url::parse(&s).map_err(serde::de::Error::custom)?;
        Ok(ShardUrl(url))
    }
}
impl<'de> Deserialize<'de> for TariffId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = u64::deserialize(d)?;
        Ok(TariffId(v))
    }
}
impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = u64::deserialize(d)?;
        Ok(Money(v))
    }
}


impl Default for Money {
    fn default() -> Self { Money(0) }
}
impl Default for TariffId {
    fn default() -> Self { TariffId(0) }
}


fn parse_duration_str(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| e.to_string())
}

#[derive(Debug, Clone)]
pub struct HumanDuration(pub Duration);

impl Default for HumanDuration {
    fn default() -> Self { HumanDuration(Duration::from_secs(0)) }
}

impl<'de> Deserialize<'de> for HumanDuration {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let dur = parse_duration_str(&s).map_err(serde::de::Error::custom)?;
        Ok(HumanDuration(dur))
    }
}


#[derive(Debug, Deserialize, Default)]
pub struct Request {
    #[serde(default, alias = "userId", alias = "user_id", alias = "user-id")]
    pub user_id: Option<UserId>,

    #[serde(default, alias = "shardUrl", alias = "shard_url", alias = "shard-url")]
    pub shard: Option<ShardUrl>,

    #[serde(default, alias = "createdAt", alias = "created_at")]
    pub created_at: Option<DateTime<Utc>>,

    #[serde(default)]
    pub ttl: HumanDuration,

    #[serde(default, alias = "tariffId", alias = "tariff_id")]
    pub tariff_id: Option<TariffId>,

    #[serde(default)]
    pub amount: Money,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_ok() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../request.json");
        let data = fs::read_to_string(path).expect("request.json");
        let req: Request = serde_json::from_str(&data).expect("parse");

        assert!(req.ttl.0.as_secs() >= 0);
        assert!(req.amount.0 >= 0);
    }
}
