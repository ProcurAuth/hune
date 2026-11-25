# Hune configuration of migrate-store for the compose example with PostgreSQL (17.7, 5.25).
# It is config.hcl with the owner role in two keys of storage.postgres.
# PostgreSQL storage, access mode open, no telemetry. Each key that applies is in the file.

server {
  listen                  = "0.0.0.0:8080"
  ops_listen              = "0.0.0.0:9090"
  base_path               = "/"
  max_connections         = 1024
  idle_timeout            = "75s"
  shutdown_grace          = "20s"
  max_request_bytes       = 4259840
  max_result_memory_bytes = 536870912
}

log {
  level = "info"
}

telemetry {
  enabled = false
}

storage {
  postgres {
    host              = "postgres"
    port              = 5432
    database          = "hune"
    user              = "hune_owner"
    password_file     = "/run/secrets/pg_owner_password"
    sslmode           = "disable"
    max_connections   = 8
    connect_timeout   = "5s"
    statement_timeout = "10s"
    poll_interval     = "5s"
    primary           = true
    history_keep      = 100
  }
}

access {
  mode = "open"
}

clickhouse "prod" {
  label              = "Production"
  url                = "https://clickhouse.example.internal:8443"
  user               = "hune"
  password_file      = "/run/secrets/clickhouse_password"
  database           = "analytics"
  connect_timeout    = "5s"
  max_execution_time = "60s"
  max_result_rows    = 100000
  max_result_bytes   = 33554432
  max_query_size     = 1048576
  max_concurrent     = 16
  queue_wait         = "10s"

  tls {
    ca_file     = "/run/secrets/clickhouse_ca.pem"
    server_name = "clickhouse.example.internal"
  }
}
