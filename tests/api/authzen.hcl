# Configuration of the API cases A39 to A45 (16.8): mode authzen, volume storage, fake-pdp on the
# compose network. The audience is hune-test. The sources are prod and conf. All values are
# synthetic test values. Each key that applies is in the file.

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
    rescan_interval = "1s"
    history_keep    = 100
  }
}

access {
  mode = "authzen"

  identity {
    header           = "X-Identity-Token"
    issuer           = "https://edge.example"
    audience         = "hune-test"
    public_keys_file = "/run/secrets/edge/jwks.json"
    max_age          = "5m"
    clock_skew       = "1m"
  }

  pdp {
    url             = "http://fake-pdp:8181"
    token_file      = "/run/secrets/pdp/token"
    plain_http      = true
    timeout         = "500ms"
    cache_ttl       = "30s"
    max_connections = 4
  }
}

clickhouse "prod" {
  label                      = "Production"
  url                        = "http://clickhouse:8123"
  user                       = "hune"
  password_file              = "/run/secrets/clickhouse/password"
  database                   = "analytics"
  connect_timeout            = "5s"
  max_execution_time         = "1m"
  max_result_rows            = 100000
  max_result_bytes           = 33554432
  max_query_size             = 1048576
  max_concurrent             = 16
  queue_wait                 = "5s"
  max_concurrent_per_subject = 6
}

clickhouse "conf" {
  label                      = "Conformance"
  url                        = "http://clickhouse:8123"
  user                       = "hune"
  password_file              = "/run/secrets/clickhouse/password"
  database                   = "analytics"
  connect_timeout            = "5s"
  max_execution_time         = "30s"
  max_result_rows            = 50000
  max_result_bytes           = 33554432
  max_query_size             = 1048576
  max_concurrent             = 16
  queue_wait                 = "5s"
  max_concurrent_per_subject = 6
}
