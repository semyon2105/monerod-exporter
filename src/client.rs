use serde_json::json;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt;

#[derive(Clone, Debug, Deserialize)]
pub struct InfoResponse {
    pub block_size_limit: u64,
    pub block_size_median: u64,
    pub block_weight_limit: u64,
    pub block_weight_median: u64,
    pub cumulative_difficulty: u64,
    pub database_size: u64,
    pub difficulty: u64,
    pub free_space: u64,
    pub grey_peerlist_size: u64,
    pub height: u64,
    pub incoming_connections_count: u64,
    pub offline: bool,
    pub outgoing_connections_count: u64,
    pub rpc_connections_count: u64,
    pub synchronized: bool,
    pub target: u64,
    pub target_height: u64,
    pub tx_count: u64,
    pub tx_pool_size: u64,
    pub untrusted: bool,
    pub white_peerlist_size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BlockHeadersRangeRequest {
    pub start_height: u64,
    pub end_height: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BlockHeader {
    pub block_size: u64,
    pub num_txes: u64,
    pub orphan_status: bool,
    pub reward: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BlockHeadersRangeResponse {
    pub headers: Vec<BlockHeader>,
    pub untrusted: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TransactionPoolStats {
    pub bytes_max: u64,
    pub bytes_med: u64,
    pub bytes_min: u64,
    pub bytes_total: u64,
    pub num_10m: u64,
    pub num_double_spends: u64,
    pub num_failing: u64,
    pub num_not_relayed: u64,
    pub oldest: u64,
    pub txs_total: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TransactionPoolStatsResponse {
    pub pool_stats: TransactionPoolStats,
    pub untrusted: bool,
}

#[derive(Clone, Debug)]
pub struct Client {
    http_client: reqwest::Client,
    base_url: String,
}

#[derive(Debug)]
pub enum ClientError {
    HttpClient(reqwest::Error),
    ResponseDeserialization(serde_json::Error),
    NoResult,
    UnexpectedStatus,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::HttpClient(e) => {
                write!(f, "HTTP client error: {}", e)
            },
            ClientError::ResponseDeserialization(e) => {
                write!(f, "response deserialization error: {}", e)
            },
            ClientError::NoResult => f.write_str("result not found in the response"),
            ClientError::UnexpectedStatus => f.write_str("unexpected or missing status"),
        }
    }
}

impl Client {
    async fn call<S, B, R>(
        &self,
        result_selector: S,
        path: &str,
        body: B,
    ) -> Result<R, ClientError>
    where
        S: FnOnce(serde_json::Value) -> Option<serde_json::Value>,
        B: Serialize,
        R: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url.clone(), path);
        let response = self.http_client
            .post(url).json(&body).send().await.map_err(ClientError::HttpClient)?
            .json::<serde_json::Value>().await.map_err(ClientError::HttpClient)?;

        let result = result_selector(response).ok_or(ClientError::NoResult)?;

        let status = result.get("status").and_then(|v| v.as_str());
        if status != Some("OK") {
            return Err(ClientError::UnexpectedStatus);
        }

        serde_json::from_value(result.clone()).map_err(ClientError::ResponseDeserialization)
    }

    fn get_json_rpc_result(value: serde_json::Value) -> Option<serde_json::Value> {
        value.get("result").cloned()
    }

    async fn call_json_rpc<B, R>(&self, method: &str, body: B) -> Result<R, ClientError>
    where
        B: Serialize,
        R: DeserializeOwned,
    {
        let body = json!({
            "method": method,
            "params": body,
        });
        self.call(Self::get_json_rpc_result, "/json_rpc", body).await
    }

    async fn call_rpc<B, R>(&self, path: &str, body: B) -> Result<R, ClientError>
    where
        B: Serialize,
        R: DeserializeOwned,
    {
        self.call(Some, path, body).await
    }

    pub fn new(http_client: reqwest::Client, base_url: String) -> Client {
        Client {
            http_client,
            base_url,
        }
    }

    pub async fn get_info(&self) -> Result<InfoResponse, ClientError> {
        self.call_json_rpc("get_info", json!({})).await
    }

    pub async fn get_block_headers_range(
        &self,
        req: BlockHeadersRangeRequest,
    ) -> Result<BlockHeadersRangeResponse, ClientError> {
        self.call_json_rpc("get_block_headers_range", req).await
    }

    pub async fn get_transaction_pool_stats(
        &self
    ) -> Result<TransactionPoolStatsResponse, ClientError> {
        self.call_rpc("/get_transaction_pool_stats", json!({})).await
    }
}
