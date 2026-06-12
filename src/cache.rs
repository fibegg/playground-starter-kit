use anyhow::Result;
use redis::AsyncCommands;
use serde::{Serialize, de::DeserializeOwned};

pub async fn get_json<T: DeserializeOwned>(client: &redis::Client, key: &str) -> Result<Option<T>> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    let value: Option<String> = conn.get(key).await?;
    value
        .map(|body| serde_json::from_str(&body).map_err(Into::into))
        .transpose()
}

pub async fn set_json<T: Serialize>(
    client: &redis::Client,
    key: &str,
    value: &T,
    ttl_seconds: u64,
) -> Result<()> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    let body = serde_json::to_string(value)?;
    let _: () = conn.set_ex(key, body, ttl_seconds).await?;
    Ok(())
}

pub async fn delete(client: &redis::Client, key: &str) -> Result<()> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    let _: () = conn.del(key).await?;
    Ok(())
}
