# Hune configuration, profile distributed: PostgreSQL storage, access mode authzen, no telemetry.
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
  backend = "postgres"

  postgres {
    host              = "hune-pg-rw"
    port              = 5432
    database          = "hune"
    user_file         = "/run/secrets/pg/username"
    password_file     = "/run/secrets/pg/password"
    sslmode           = "verify-full"
    max_connections   = 8
    connect_timeout   = "5s"
    statement_timeout = "10s"
    poll_interval     = "5s"
    history_keep      = 100

    tls {
      ca_file     = "/run/secrets/pg-ca/ca.crt"
      server_name = "hune-pg-rw"
    }
  }
}

access {
  mode = "authzen"

  identity {
    header           = "X-Identity-Token"
    issuer           = "https://edge.example"
    audience         = "dashboards"
    public_keys_file = "/run/secrets/edge/jwks.json"
    max_age          = "5m"
    clock_skew       = "1m"
  }

  pdp {
    url             = "https://pdp.example:4182"
    token_file      = "/run/secrets/pdp/token"
    ca_file         = "/run/secrets/pdp/ca.pem"
    plain_http      = false
    timeout         = "500ms"
    cache_ttl       = "30s"
    max_connections = 4
  }
}

clickhouse "prod" {
  label                      = "Production"
  url                        = "https://clickhouse:8443"
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

  tls {
    ca_file     = "/run/secrets/clickhouse/ca.pem"
    server_name = "clickhouse"
  }
}
