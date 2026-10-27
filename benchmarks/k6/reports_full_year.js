// README §13 load scenario 4 — full-year revenue report (by-day), the read
// heaviest report the server serves (§7 reports read daily_revenue only).
//
//   k6 run -e BASE_URL=http://localhost:8080 -e HOTEL=4443 \
//          -e USERNAME=loadmgr -e PASSWORD=... reports_full_year.js
//
// Threshold (README §13): p95 < 500 ms
import http from 'k6/http';
import { check } from 'k6';

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const HOTEL = __ENV.HOTEL || '4443';
const USERNAME = __ENV.USERNAME || 'loadmgr';
const PASSWORD = __ENV.PASSWORD;
const N = parseInt(__ENV.N || '200', 10);

export const options = {
  scenarios: {
    report: {
      executor: 'shared-iterations',
      vus: parseInt(__ENV.VUS || '16', 10),
      iterations: N,
      maxDuration: '5m',
    },
  },
  thresholds: { http_req_duration: ['p(95)<500'] },
  noCookies: true,
};

export function setup() {
  if (!PASSWORD) {
    throw new Error('PASSWORD env var is required (never stored in the repo)');
  }
  const res = http.post(
    `${BASE_URL}/api/v1/auth/login`,
    JSON.stringify({ hotel_id: HOTEL, username: USERNAME, password: PASSWORD }),
    { headers: { 'Content-Type': 'application/json' } },
  );
  check(res, { 'login ok': (r) => r.status === 200 });
  return { token: res.json('data.token') };
}

export default function (data) {
  const from = new Date();
  const to = new Date(from.getTime() + 365 * 86400000);
  const url = `${BASE_URL}/api/v1/reports/by-day?from=${from.toISOString().slice(0, 10)}&to=${to.toISOString().slice(0, 10)}`;
  const res = http.get(url, { headers: { Authorization: `Bearer ${data.token}` } });
  check(res, {
    'report 200': (r) => r.status === 200,
    'report has rows': (r) => Array.isArray(r.json('data')) && r.json('data').length > 0,
  });
}
