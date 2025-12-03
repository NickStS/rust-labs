use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use url::Url;
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseType {
    Success,
    Error,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PublicTariff {
    pub id: u64,
    pub price: u64,
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
    pub description: String,
}
 
#[derive(Debug, Deserialize, Serialize)]
pub struct PrivateTariff {
    pub client_price: u64,
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Stream {
    pub user_id: Uuid,
    pub is_private: bool,
    pub settings: u64,
    pub shard_url: Url,
    pub public_tariff: PublicTariff,
    pub private_tariff: PrivateTariff,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Gift {
    pub id: u64,
    pub price: u64,
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Debug {
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    #[serde(rename = "type")]
    pub response_type: ResponseType,
    pub stream: Stream,
    pub gifts: Vec<Gift>,
    pub debug: Debug,
}

impl Request {
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_request_json() {
        let json_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../request.json");
        let json_data = fs::read_to_string(json_path).expect("Failed to read request.json");
        
        let request = Request::from_json(&json_data).expect("Failed to parse JSON");
        
        assert_eq!(request.stream.user_id.to_string(), "8d234120-0bda-49b2-b7e0-fbd3912f6cbf");
        assert_eq!(request.stream.is_private, false);
        assert_eq!(request.stream.settings, 45345);
        assert_eq!(request.stream.shard_url.as_str(), "https://n3.example.com/sapi");
        assert_eq!(request.stream.public_tariff.id, 1);
        assert_eq!(request.stream.public_tariff.price, 100);
        assert_eq!(request.stream.public_tariff.duration.as_secs(), 3600);
        assert_eq!(request.stream.private_tariff.client_price, 250);
        assert_eq!(request.stream.private_tariff.duration.as_secs(), 60);
        assert_eq!(request.gifts.len(), 2);
        assert_eq!(request.gifts[0].id, 1);
        assert_eq!(request.gifts[1].price, 3);
        assert_eq!(request.debug.duration.as_millis(), 234);
    }

    #[test]
    fn convert_to_toml() {
        let json_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../request.json");
        let json_data = fs::read_to_string(json_path).expect("Failed to read request.json");
        
        let request = Request::from_json(&json_data).expect("Failed to parse JSON");
        let toml_str = request.to_toml().expect("Failed to convert to TOML");
        
        assert!(toml_str.contains("response_type"));
        assert!(toml_str.contains("user_id"));
        assert!(toml_str.contains("8d234120-0bda-49b2-b7e0-fbd3912f6cbf"));
        
        println!("\n=== TOML Output ===\n{}", toml_str);
    }
}
