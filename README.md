# monerod-exporter

Exports metrics and stats from a Monero daemon instance in Prometheus exposition format.

Exported data:

- Instance metrics (database size, connections, etc.) 
- Monero network metrics (difficulty, height, total txs, etc.)
- Transaction pool stats
- Block stats over last N blocks

## Configuration

`monerod` instance must have unrestricted RPC enabled for the exporter to work correctly.

The exporter doesn't usually require additional configuration if deployed alongside `monerod` on the same host. Default configuration binds the exporter to `[::]:8080` and assumes the daemon RPC to be available at `http://localhost:18081`.

To use a custom config, run:

```
./monerod-exporter -c config.toml
```

Config values can also be set using environment variables:

```
MONEROD_EXPORTER_SERVER__HOST=[::]:1234 ./monerod-exporter
```

See [config.toml](./config.toml) for all available settings.

## Dashboards

Pre-made Grafana v7.5+ [dashboards](./dashboards) that are set up to work with a Prometheus datasource. When importing the network metrics dashboard, set the `first_timestamp` variable to the timestamp of the first scrape.
