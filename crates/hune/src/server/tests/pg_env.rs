// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers of the PostgreSQL tests: the connection data from the environment,
//! the databases with the grants of 5.25, and the configuration files for the binary.
//!
//! The file uses only the standard library. The integration test `tests/pg_cli.rs` includes it
//! with `#[path]`. The tests that use a server are ignored: `tests/ci/pgwire.sh` starts
//! PostgreSQL 15 and 18 in containers, sets the variables below and runs the ignored tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The directory of the configuration fixtures.
pub const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/config");

fn var(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("the variable {name} is not set: run the PostgreSQL tests with tests/ci/pgwire.sh"))
}

/// The two roles of 5.25.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// The owner role: `migrate-store` only.
    Owner,
    /// The application role: `serve` and the other commands.
    App,
}

/// The connection data of the PostgreSQL container.
#[derive(Clone, Debug)]
pub struct PgEnv {
    pub docker: String,
    pub container: String,
    pub host: String,
    pub port: u16,
    pub owner_user: String,
    pub owner_password: String,
    pub app_user: String,
    pub app_password: String,
    pub ca_file: PathBuf,
    pub server_name: String,
}

impl PgEnv {
    /// Reads the variables of `tests/ci/pgwire.sh`. It panics when one is missing.
    pub fn from_env() -> PgEnv {
        PgEnv {
            docker: var("HUNE_PGW_DOCKER"),
            container: var("HUNE_PGW_CONTAINER"),
            host: var("HUNE_PGW_HOST"),
            port: var("HUNE_PGW_PORT").parse().expect("HUNE_PGW_PORT"),
            owner_user: var("HUNE_PGW_OWNER_USER"),
            owner_password: var("HUNE_PGW_OWNER_PASSWORD"),
            app_user: var("HUNE_PGW_APP_USER"),
            app_password: var("HUNE_PGW_APP_PASSWORD"),
            ca_file: PathBuf::from(var("HUNE_PGW_CA_FILE")),
            server_name: var("HUNE_PGW_SERVER_NAME"),
        }
    }

    /// The user and the password of a role.
    pub fn credentials(&self, role: Role) -> (&str, &str) {
        match role {
            Role::Owner => (&self.owner_user, &self.owner_password),
            Role::App => (&self.app_user, &self.app_password),
        }
    }

    /// Runs SQL as the administrator in a database of the container. It panics on an error.
    pub fn admin(&self, database: &str, sql: &str) {
        let out = Command::new(&self.docker)
            .args(["exec", "-i", &self.container, "psql", "-U", "postgres", "-X", "-q", "-v", "ON_ERROR_STOP=1"])
            .args(["-d", database, "-c", sql])
            .output()
            .expect("run docker exec psql");
        assert!(
            out.status.success(),
            "psql failed for {sql:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Makes an empty database with the grants of 5.25. A database of the same name is dropped first.
    /// The name is unique for the run: it holds the process id.
    pub fn fresh_database(&self, label: &str) -> String {
        let name = format!("vgw_{label}_{}", std::process::id());
        self.admin("postgres", &format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"));
        self.admin("postgres", &format!("CREATE DATABASE \"{name}\""));
        let (o, p) = (&self.owner_user, &self.app_user);
        self.admin(
            &name,
            &format!(
                "GRANT CONNECT ON DATABASE \"{name}\" TO \"{o}\"; GRANT CREATE, USAGE ON SCHEMA public TO \"{o}\"; \
                 GRANT CONNECT ON DATABASE \"{name}\" TO \"{p}\"; GRANT USAGE ON SCHEMA public TO \"{p}\"; \
                 ALTER DEFAULT PRIVILEGES FOR ROLE \"{o}\" IN SCHEMA public \
                 GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO \"{p}\""
            ),
        );
        name
    }

    /// Writes a complete configuration file for the binary. `tls` selects `verify-full`. The
    /// ports are those of `server.listen` and `server.ops_listen`, and 0 gives a free port to the
    /// server (the tests of the commands never bind them).
    pub fn write_config(
        &self,
        dir: &Path,
        name: &str,
        database: &str,
        role: Role,
        tls: bool,
        ports: (u16, u16),
    ) -> PathBuf {
        let (user, password) = self.credentials(role);
        let pw_file = dir.join(format!("{name}.pgpw"));
        std::fs::write(&pw_file, format!("{password}\n")).unwrap();
        std::fs::write(dir.join("ch.pw"), "chpass-marker\n").unwrap();
        std::fs::copy(Path::new(FIXTURES).join("ca-a.pem"), dir.join("ch-ca.pem")).unwrap();
        let tls_block = if tls {
            let ca = dir.join("pg-ca.pem");
            std::fs::copy(&self.ca_file, &ca).unwrap();
            format!(
                "\n    tls {{\n      ca_file     = \"{}\"\n      server_name = \"{}\"\n    }}\n",
                ca.display(),
                self.server_name
            )
        } else {
            String::new()
        };
        let storage = format!(
            "storage {{\n  backend = \"postgres\"\n\n  postgres {{\n    host              = \"{host}\"\n    \
             port              = {port}\n    database          = \"{database}\"\n    user              = \"{user}\"\n    \
             password_file     = \"{pw}\"\n    sslmode           = \"{mode}\"\n    max_connections   = 8\n    \
             connect_timeout   = \"5s\"\n    statement_timeout = \"10s\"\n    poll_interval     = \"1s\"\n    \
             history_keep      = 100\n{tls_block}  }}\n}}\n\n",
            host = self.host,
            port = self.port,
            pw = pw_file.display(),
            mode = if tls { "verify-full" } else { "disable" },
        );
        let text = std::fs::read_to_string(Path::new(FIXTURES).join("local.hcl")).unwrap();
        let (head, rest) = text.split_once("storage {").expect("the fixture has a block storage");
        let (_, tail) = rest.split_once("\naccess {").expect("the fixture has a block access");
        let text = format!("{head}{storage}access {{{tail}")
            .replace("/run/secrets/clickhouse/password", &dir.join("ch.pw").display().to_string())
            .replace("/run/secrets/clickhouse/ca.pem", &dir.join("ch-ca.pem").display().to_string())
            .replace("0.0.0.0:8080", &format!("127.0.0.1:{}", ports.0))
            .replace("0.0.0.0:9090", &format!("127.0.0.1:{}", ports.1))
            .replace("\"30s\"", "\"8s\"");
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
}
