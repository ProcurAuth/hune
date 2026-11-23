// Generate the synthetic dashboard of the load test (16.16) as a volume folder.
// Use: node tests/load/gen.mjs VOLUME_DIR [PANELS]
// The volume gets `load/_folder.json` and `load/load45.json`. The dashboard has PANELS panels
// (default 45), alternately `table` and `timeseries`. Each panel runs `SELECT number FROM numbers(100)`.
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const volume = process.argv[2];
const count = Number(process.argv[3] ?? 45);
if (!volume || !Number.isInteger(count) || count < 1) {
  console.error('use: gen.mjs VOLUME_DIR [PANELS]');
  process.exit(2);
}

const SQL = 'SELECT number FROM numbers(100)';

function panel(id) {
  const isTable = id % 2 === 1;
  const row = Math.floor((id - 1) / 3);
  const col = (id - 1) % 3;
  const base = {
    id,
    type: isTable ? 'table' : 'timeseries',
    title: `Panel ${id}`,
    description: '',
    transparent: false,
    grid: { x: col * 8, y: row * 8, w: 8, h: 8 },
    data: {
      query: {
        source: 'prod',
        format: isTable ? 'table' : 'time_series',
        sql: [SQL],
        min_interval: null,
        max_data_points: null,
      },
    },
    transforms: [],
    fields: { defaults: {}, overrides: [] },
    links: [],
  };
  if (isTable) {
    base.options = { show_header: true, sort: [] };
  } else {
    base.options = {
      draw: 'line',
      interpolation: 'linear',
      line_width: 1,
      fill_opacity: 0,
      show_points: 'auto',
      point_size: 5,
      span_nulls: false,
      stacking: 'none',
      y_scale: 'linear',
      legend: { show: true, placement: 'bottom', mode: 'list', values: [] },
      tooltip: { mode: 'single' },
    };
  }
  return base;
}

const panels = [];
for (let id = 1; id <= count; id += 1) panels.push(panel(id));

const document = {
  format_version: 3,
  uid: 'load45',
  title: 'Load test, synthetic dashboard',
  description: 'Synthetic dashboard of the load test.',
  tags: [],
  time: { from: 'now-6h', to: 'now', timezone: 'utc', refresh: null },
  cursor_sync: 'off',
  links: [],
  variables: [],
  sections: [{ title: null, collapsed: false, panels }],
};

mkdirSync(join(volume, 'load'), { recursive: true });
writeFileSync(
  join(volume, 'load', '_folder.json'),
  JSON.stringify({ format_version: 1, title: 'Load' }, null, 2) + '\n',
);
writeFileSync(join(volume, 'load', 'load45.json'), JSON.stringify(document, null, 2) + '\n');
console.log(`wrote ${count} panels in ${join(volume, 'load')}`);
