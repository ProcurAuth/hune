# Configuration of the run of the series U (11.32, 16.12): mode authzen, volume on the fixture.
# The runner is the edge and the PDP on 127.0.0.1:8181. The paths are the paths in the container.
# All values are synthetic test values.

server {
  listen                  = "127.0.0.1:8080"
  ops_listen              = "127.0.0.1:9090"
  base_path               = "/"
  editing                 = true
  max_connections         = 256
  idle_timeout            = "75s"
  shutdown_grace          = "10s"
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
    path            = "/data/dashboards"
    read_only       = false
    rescan_interval = "5s"
    history_keep    = 100
  }
}

access {
  mode = "authzen"

  identity {
    header           = "X-Identity-Token"
    issuer           = "https://edge.example"
    audience         = "hune-conf"
    public_keys_file = "/conformance/jwks.json"
    max_age          = "5m"
    clock_skew       = "30s"
  }

  pdp {
    url             = "http://127.0.0.1:8181"
    token_file      = "/conformance/pdp-credential.txt"
    plain_http      = true
    timeout         = "2s"
    cache_ttl       = "0s"
    max_connections = 4
  }
}

clickhouse "conf" {
  label                      = "Conformance"
  url                        = "http://127.0.0.1:8123"
  user                       = "hune"
  password_file              = "/conformance/clickhouse_password"
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
