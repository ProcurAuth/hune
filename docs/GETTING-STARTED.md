# Getting started

This test takes five minutes. It runs a local ClickHouse with synthetic data, then it runs Hune with the demo dashboards. Everything stays on `127.0.0.1`. You need Docker, `git` and `curl`. The test pushes no image.

## 1. Get the demo files

The folder `examples/demo/` of the repository holds the demo data, a configuration and the demo dashboards.

```sh
git clone --depth 1 https://github.com/ProcurAuth/hune.git
```

## 2. Start ClickHouse and load the data

```sh
docker run -d --rm --name hune-demo-ch -p 127.0.0.1:8123:8123 clickhouse/clickhouse-server:26.8.34.13
sleep 15
docker exec -i hune-demo-ch clickhouse-client --multiquery < hune/examples/demo/init.sql
```

The script `init.sql` makes the database `demo` with synthetic tables. It also makes the user `hune_reader`. This user is read-only: it has the profile `readonly = 2`, `MAX` limits and `SELECT` grants only. The password of the user is a public demo value.

## 3. Download Hune

The release has a static binary for Linux amd64. Check the archive against `SHA256SUMS`.

```sh
curl -fsSLO https://github.com/ProcurAuth/hune/releases/download/v0.3.0/hune_0.3.0_linux_amd64.tar.gz
curl -fsSLO https://github.com/ProcurAuth/hune/releases/download/v0.3.0/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
tar -xzf hune_0.3.0_linux_amd64.tar.gz
```

The archive also holds `config.reference.hcl` (a complete configuration file), the folder `deploy/` and a `Dockerfile` for a local image.

## 4. Check the configuration

The demo configuration uses the volume store in `/tmp/hune-demo/dashboards`. Copy the demo dashboards there. The password comes from the environment variable that the key `password_env` names.

```sh
mkdir -p /tmp/hune-demo && cp -R hune/examples/demo/dashboards /tmp/hune-demo/dashboards
export HUNE_DEMO_CH_PASSWORD=hune-demo-password
hune_0.3.0_linux_amd64/hune check-config -c hune/examples/demo/hune.hcl
hune_0.3.0_linux_amd64/hune print-config -c hune/examples/demo/hune.hcl
```

`check-config` finds each error and sends nothing on the network. `print-config` shows the value that applies for each key and the source of the value. A secret shows as `<hidden>`.

## 5. Run Hune

```sh
hune_0.3.0_linux_amd64/hune serve -c hune/examples/demo/hune.hcl
```

Open `http://127.0.0.1:8080/`. The list shows the demo folders. Open a dashboard and change the time range. Pick a value of a variable. Open the inspect view of a panel to see the SQL that the server sent. The page can write: the switch Edit opens the editor.

The ops port `127.0.0.1:9090` serves `/livez`, `/readyz` and `/metrics`.

## 6. Stop and clean up

Press Ctrl+C to stop Hune. Then remove the container and the files.

```sh
docker stop hune-demo-ch
rm -rf /tmp/hune-demo
```

## Next steps

- Read the [configuration](CONFIGURATION.md) and write your own file with `hune print-reference --format hcl --profile local`.
- Use a read-only account for each source. [Security model](SECURITY-MODEL.md) tells why.
- Import dashboards from Grafana with `hune import-grafana`. Section 15 of the [specification](SPECIFICATION.md) has the steps.
- Add access control with [ProcurAuth](ACCESS.md), or put a reverse proxy that signs users in in front of Hune.
