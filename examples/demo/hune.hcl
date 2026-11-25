# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Configuration of the Hune demo. Every value is a local demo value.
# Hune needs an absolute path for the volume. Copy the folder `dashboards` to /tmp/hune-demo first.
# The ClickHouse password comes from the environment variable that `password_env` names.

server {
  listen                  = "127.0.0.1:8080"
  ops_listen              = "127.0.0.1:9090"
  base_path               = "/"
  query_inspect           = true
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
  volume "main" {
    path            = "/tmp/hune-demo/dashboards"
    primary         = true
    rescan_interval = "5s"
    history_keep    = 100
  }
}

access {
  mode = "open"
}

clickhouse "demo" {
  label                      = "Demo"
  url                        = "http://127.0.0.1:8123"
  user                       = "hune_reader"
  password_env               = "HUNE_DEMO_CH_PASSWORD"
  database                   = "demo"
  connect_timeout            = "5s"
  max_execution_time         = "30s"
  max_result_rows            = 100000
  max_result_bytes           = 33554432
  max_query_size             = 1048576
  max_concurrent             = 16
  queue_wait                 = "5s"
  adhoc_columns              = ["service", "host", "region"]
}
