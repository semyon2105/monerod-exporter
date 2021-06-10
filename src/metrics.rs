use std::{fmt, sync::RwLock, time::Duration};
use tokio::{time::interval, try_join};
use tracing::{error, info, instrument};

use crate::{
    client::{BlockHeader, BlockHeadersRangeRequest, Client, ClientError},
    prometheus::{Metric, render_metrics},
};

#[derive(Clone, Debug)]
pub struct Exporter {
    client: Client,
    max_block_span: u32,
    block_spans: Vec<u32>,
}

#[derive(Debug)]
pub enum ExportError {
    Client(ClientError),
    Renderer(fmt::Error),
    Untrusted,
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::Client(e) => {
                write!(f, "monero RPC client error: {}", e)
            },
            ExportError::Renderer(e) => {
                write!(f, "rendering error: {}", e)
            },
            ExportError::Untrusted => f.write_str("received an untrusted response from node"),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct BlocksMetrics {
    avg_txes: f64,
    max_txes: f64,
    avg_reward: f64,
    max_reward: f64,
    avg_size: f64,
    max_size: f64,
}

impl Exporter {
    pub fn new(client: Client, block_spans: Vec<u32>) -> Exporter {
        let block_spans =
            if block_spans.is_empty() {
                vec![1]
            } else {
                block_spans
            };

        let max_block_span = block_spans.iter().max().cloned().unwrap_or(1);

        Exporter {
            client,
            max_block_span,
            block_spans,
        }
    }

    fn get_blocks_metrics(headers: &[BlockHeader], count: u32) -> BlocksMetrics {
        let non_orphan_blocks =
            headers.iter()
                .take(count as usize)
                .filter(|h| !h.orphan_status)
                .collect::<Vec<_>>();

        let blocks_metrics = non_orphan_blocks.iter()
            .fold(BlocksMetrics::default(), |acc, block| BlocksMetrics {
                avg_txes: acc.avg_txes + block.num_txes as f64,
                max_txes: acc.max_txes.max(block.num_txes as f64),
                avg_reward: acc.avg_reward + block.reward as f64,
                max_reward: acc.max_reward.max(block.reward as f64),
                avg_size: acc.avg_size + block.block_size as f64,
                max_size: acc.max_size.max(block.block_size as f64),
            });

        BlocksMetrics {
            avg_txes: blocks_metrics.avg_txes / non_orphan_blocks.len() as f64,
            avg_reward: blocks_metrics.avg_reward / non_orphan_blocks.len() as f64,
            avg_size: blocks_metrics.avg_size / non_orphan_blocks.len() as f64,
            ..blocks_metrics
        }
    }

    #[instrument(name = "export_metrics", skip(self))]
    pub async fn export(&self) -> Result<String, ExportError> {
        let info = self.client.get_info().await.map_err(ExportError::Client)?;

        // assuming all other responses will have the same value for "untrusted"
        if info.untrusted {
            return Err(ExportError::Untrusted);
        }

        let mut metrics = Vec::with_capacity(100);

        let mut push_metric = |name: &str, value| {
            metrics.push(Metric::new_gauge(name, value));
        };

        // Node metrics
        push_metric("monero_node_database_size", info.database_size as f64);
        push_metric("monero_node_free_space", info.free_space as f64);
        push_metric("monero_node_grey_peerlist_size", info.grey_peerlist_size as f64);
        push_metric("monero_node_incoming_connections_count", info.incoming_connections_count as f64);
        push_metric("monero_node_offline", info.offline as u8 as f64);
        push_metric("monero_node_outgoing_connections_count", info.outgoing_connections_count as f64);
        push_metric("monero_node_rpc_connections_count", info.rpc_connections_count as f64);
        push_metric("monero_node_synchronized", info.synchronized as u8 as f64);
        push_metric("monero_node_white_peerlist_size", info.white_peerlist_size as f64);

        if !info.synchronized {
            info!("node is not synchronized yet - skipped exporting tx pool and network metrics");

            let mut s = String::new();
            return render_metrics(metrics.iter(), &mut s)
                .map(|_| s)
                .map_err(ExportError::Renderer);
        }

        let block_headers_req = BlockHeadersRangeRequest {
            start_height: info.height.checked_sub(self.max_block_span.into()).unwrap_or(0),
            end_height: info.height.checked_sub(1).unwrap_or(0),
        };
        let (tx_pool_stats, block_headers) = try_join!(
            self.client.get_transaction_pool_stats(),
            self.client.get_block_headers_range(block_headers_req),
        ).map_err(ExportError::Client)?;

        let pool_stats = tx_pool_stats.pool_stats;
        let block_headers = block_headers.headers;

        // Node metrics - transaction pool
        push_metric("monero_txpool_bytes_max", pool_stats.bytes_max as f64);
        push_metric("monero_txpool_bytes_med", pool_stats.bytes_med as f64);
        push_metric("monero_txpool_bytes_min", pool_stats.bytes_min as f64);
        push_metric("monero_txpool_bytes_total", pool_stats.bytes_total as f64);
        push_metric("monero_txpool_double_spends", pool_stats.num_double_spends as f64);
        push_metric("monero_txpool_txs_failing", pool_stats.num_failing as f64);
        push_metric("monero_txpool_txs_not_relayed", pool_stats.num_not_relayed as f64);
        push_metric("monero_txpool_oldest_tx", pool_stats.oldest as f64);
        push_metric("monero_txpool_txs_above_10min", pool_stats.num_10m as f64);
        push_metric("monero_txpool_txs_total", pool_stats.txs_total as f64);

        // Network metrics
        push_metric("monero_network_block_size_limit", info.block_size_limit as f64);
        push_metric("monero_network_block_size_median", info.block_size_median as f64);
        push_metric("monero_network_block_weight_limit", info.block_weight_limit as f64);
        push_metric("monero_network_block_weight_median", info.block_weight_median as f64);
        push_metric("monero_network_cumulative_difficulty", info.cumulative_difficulty as f64);
        push_metric("monero_network_difficulty", info.difficulty as f64);
        push_metric("monero_network_height", info.height as f64);
        push_metric("monero_network_target", info.target as f64);
        push_metric("monero_network_target_height", info.target_height as f64);
        push_metric("monero_network_tx_count", info.tx_count as f64);

        let blocks_metrics = self.block_spans.iter()
            .map(|count| (count.to_string(), Exporter::get_blocks_metrics(&block_headers, *count)))
            .collect::<Vec<_>>();

        let mut push_blocks_metric = |name: &str, metric_selector: fn(BlocksMetrics) -> f64| {
            let values = blocks_metrics.clone().into_iter()
                .map(|(count, m)| (count, metric_selector(m)));

            metrics.push(Metric::new_gauge_with_label_values(name, "block_count", values));
        };

        // Network metrics - blocks
        push_blocks_metric("monero_blocks_avg_txes", |m| m.avg_txes);
        push_blocks_metric("monero_blocks_max_txes", |m| m.max_txes);
        push_blocks_metric("monero_blocks_avg_reward", |m| m.avg_reward);
        push_blocks_metric("monero_blocks_max_reward", |m| m.max_reward);
        push_blocks_metric("monero_blocks_avg_size", |m| m.avg_size);
        push_blocks_metric("monero_blocks_max_size", |m| m.max_size);

        let mut s = String::new();
        render_metrics(metrics.iter(), &mut s)
            .map(|_| s)
            .map_err(ExportError::Renderer)
    }
}

#[derive(Debug)]
pub struct Publisher {
    exporter: Exporter,
    refresh_interval: Duration,
    rendered_metrics: RwLock<Option<String>>,
}

impl Publisher {
    pub fn new(exporter: Exporter, refresh_interval: Duration) -> Publisher {
        Publisher {
            exporter,
            refresh_interval,
            rendered_metrics: RwLock::new(None),
        }
    }

    pub fn get_metrics(&self) -> Option<String> {
        self.rendered_metrics.read().unwrap().clone()
    }

    pub async fn run(&self) -> ! {
        let mut interval = interval(self.refresh_interval);
        loop {
            interval.tick().await;

            let result = self.exporter.export().await;

            let result = match result {
                Ok(r) => Some(r),
                Err(e) => {
                    error!("{}", e);
                    None
                },
            };

            {
                let mut rendered_metrics = self.rendered_metrics.write().unwrap();
                *rendered_metrics = result;
            }
        }
    }
}
