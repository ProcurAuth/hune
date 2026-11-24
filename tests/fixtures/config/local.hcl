# Hune configuration, profile local: volume storage, access mode open, no telemetry.
# Each key that applies is in the file, except the seven keys that have a default: `server.query_inspect`, `server.repeat_max_per_panel`, `server.repeat_max_per_section`, `server.repeat_max_per_dashboard`, `server.compress_static`, `server.transform_max_cells` and `server.list_filters_min_dashboards`.

server {
  listen                  = "0.0.0.0:8080"
  ops_listen              = "0.0.0.0:9090"
  base_path               = "/"
  editing                 = true
  max_connections         = 1024
  idle_timeout            = "75s"
  shutdown_grace          = "30s"
  max_request_bytes       = 8388608
  max_result_memory_bytes = 536870912
}

log {
  level = "info"
}

telemetry {
  enabled = false
}

storage {
  backend = "volume"

  volume {
    path            = "/var/lib/hune/dashboards"
    read_only       = false
    rescan_interval = "5s"
    history_keep    = 100
  }
}

access {
  mode = "open"
}

clickhouse "prod" {
  label              = "Production"
  url                = "https://clickhouse:8443"
  user               = "hune"
  password_file      = "/run/secrets/clickhouse/password"
  database           = "analytics"
  connect_timeout    = "5s"
  max_execution_time = "1m"
  max_result_rows    = 100000
  max_result_bytes   = 33554432
  max_query_size     = 1048576
  max_concurrent     = 16
  queue_wait         = "5s"

  tls {
    ca_file     = "/run/secrets/clickhouse/ca.pem"
    server_name = "clickhouse"
  }
}
