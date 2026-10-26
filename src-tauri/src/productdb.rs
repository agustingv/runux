//! Minimal decoder for Battle.net's `product.db`.
//!
//! The file is a protobuf `Database` message written by the Battle.net Agent.
//! Only the fields the launcher needs are declared; prost skips the rest.

use prost::Message;
use std::path::Path;

#[derive(Clone, PartialEq, Message)]
pub struct Database {
    #[prost(message, repeated, tag = "1")]
    pub product_installs: Vec<ProductInstall>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProductInstall {
    #[prost(string, tag = "1")]
    pub uid: String,
    #[prost(string, tag = "2")]
    pub product_code: String,
    #[prost(message, optional, tag = "3")]
    pub settings: Option<UserSettings>,
    #[prost(message, optional, tag = "4")]
    pub cached_product_state: Option<CachedProductState>,
}

#[derive(Clone, PartialEq, Message)]
pub struct UserSettings {
    #[prost(string, tag = "1")]
    pub install_path: String,
    #[prost(string, tag = "2")]
    pub play_region: String,
    #[prost(string, tag = "10")]
    pub version_branch: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct CachedProductState {
    #[prost(message, optional, tag = "1")]
    pub base_product_state: Option<BaseProductState>,
    #[prost(message, optional, tag = "4")]
    pub update_progress: Option<UpdateProgress>,
}

#[derive(Clone, PartialEq, Message)]
pub struct BaseProductState {
    #[prost(bool, tag = "1")]
    pub installed: bool,
    #[prost(bool, tag = "2")]
    pub playable: bool,
    #[prost(bool, tag = "3")]
    pub update_complete: bool,
    #[prost(string, tag = "7")]
    pub current_version_str: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct UpdateProgress {
    #[prost(double, tag = "2")]
    pub progress: f64,
    #[prost(uint64, tag = "4")]
    pub total_to_download: u64,
    #[prost(uint64, tag = "5")]
    pub download_remaining: u64,
}

/// Returns `Ok(None)` when Battle.net has not written the database yet.
pub fn read(path: &Path) -> Result<Option<Database>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Database::decode(bytes.as_slice())
            .map(Some)
            .map_err(|e| format!("product.db ilegible: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
