# Hune configuration for the compose example with PostgreSQL (17.7).
# PostgreSQL storage, access mode open, no telemetry. Each key that applies is in the file.

server {
  listen                  = "0.0.0.0:8080"
  ops_listen              = "0.0.0.0:9090"
  base_path               = "/"
  editing                 = true
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
  backend = "postgres"

  postgres {
    host              = "postgres"
    port              = 5432
    database          = "hune"
    user              = "hune_app"
    password_file     = "/run/secrets/pg_app_password"
    sslmode           = "disable"
    max_connections   = 8
    connect_timeout   = "5s"
    statement_timeout = "10s"
    poll_interval     = "5s"
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

# A PostgreSQL data source (13.12a, section 8a). The block is a comment. To use it, remove the first "#"
# of each line, from the line `postgres "metrics" {` to its closing brace. Use the role of 8a.13, and nothing else:
# - It is a dedicated login role, and it is not the role of the store.
# - It has SELECT grants only, on the schemas that the dashboards read. It cannot write or create.
# - Point "host" at a read replica when the data permits it.
# The user name and the password come from files of a secret mount. A secret in clear text in this
# file stops the start. The label of this block must differ from the label of each clickhouse block.
#
# postgres "metrics" {
#   label                      = "Metrics"
#   host                       = "hune-pg-ro.example.internal"
#   port                       = 5432
#   database                   = "analytics"
#   user_file                  = "/run/secrets/postgres_metrics_username"
#   password_file              = "/run/secrets/postgres_metrics_password"
#   search_path                = "metrics"
#   sslmode                    = "verify-full"
#   connect_timeout            = "5s"
#   max_execution_time         = "1m"
#   max_result_rows            = 100000
#   max_result_bytes           = 33554432
#   max_query_size             = 1048576
#   max_concurrent             = 16
#   queue_wait                 = "5s"
#   # The key max_concurrent_per_subject needs access.mode = "authzen". It is forbidden in the mode "open".
#
#   tls {
#     ca_file     = "/run/secrets/postgres_metrics_ca.pem"
#     server_name = "hune-pg-ro.example.internal"
#   }
# }
