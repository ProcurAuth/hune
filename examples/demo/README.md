# Hune demo

This demo runs Hune on a local ClickHouse database. All data are synthetic.
The data cover the 24 hours before you load `init.sql`.

You need the binary `hune` (version 0.3.1 or later), `curl`, and Docker with the image
`clickhouse/clickhouse-server:26.8.34.13`. Run each command in this folder.

1. Start ClickHouse on 127.0.0.1. Then load the data and the read-only user.

   ```sh
   docker run -d --rm --name hune-demo-ch -p 127.0.0.1:8123:8123 clickhouse/clickhouse-server:26.8.34.13
   sleep 15
   docker exec -i hune-demo-ch clickhouse-client --multiquery < init.sql
   ```

2. Check the data. This command prints `4000`.

   ```sh
   docker exec hune-demo-ch clickhouse-client -q 'SELECT count() FROM demo.logs'
   ```

3. Copy the dashboards. Hune needs an absolute path for its volume.

   ```sh
   mkdir -p /tmp/hune-demo && cp -r dashboards /tmp/hune-demo/
   ```

4. Give the password of the read-only user to Hune, and start Hune.

   ```sh
   export HUNE_DEMO_CH_PASSWORD=hune-demo-password
   hune check-config -c hune.hcl && hune serve -c hune.hcl
   ```

5. Open <http://127.0.0.1:8080/> in a browser. Start with the dashboard "Service overview".

To stop the demo, press Ctrl+C in the terminal of Hune. Then run this command.
To get fresh data, run it and start again with step 1.

```sh
docker stop hune-demo-ch && rm -rf /tmp/hune-demo
```

## Files

- `init.sql`: the database `demo`, the read-only user `hune_reader` and the synthetic data.
- `hune.hcl`: the configuration of Hune (mode `open`, one volume, one ClickHouse source).
- `dashboards/`: four dashboards in three folders, in the format of the Hune volume.

The password of the read-only user is a public demo value. It protects nothing. Do not use them outside this demo.
