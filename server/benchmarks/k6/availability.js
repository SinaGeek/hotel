// README Â§13 load scenario 2 â€” availability search: 7-day windows at sliding
// offsets across the year (same endpoint as the board, smaller windows).
//
//   k6 run -e BASE_URL=http://localhost:8080 -e HOTEL=4443 \
//          -e USERNAME=loadmgr -e PASSWORD=... availability.js
//
// Threshold (README Â§13): p95 < 200 ms
import http from 'k6/http';
import { check } from 'k6';

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const HOTEL = parseInt(__ENV.HOTEL || '4443', 10);
const USERNAME = __ENV.USERNAME || 'loadmgr';
const PASSWORD = __ENV.PASSWORD;
const N = parseInt(__ENV.N || '200', 10);

export const options = {
  scenarios: {
    availability: {
      executor: 'shared-iterations',
      vus: parseInt(__ENV.VUS || '16', 10),
      iterations: N,
      maxDuration: '5m',
    },
  },
  thresholds: { http_req_duration: ['p(95)<200'] },
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
  const offset = Math.floor(Math.random() * 300);
  const from = new Date(Date.now() + offset * 86400000);            // sliding across the year
  const to = new Date(from.getTime() + 7 * 86400000);               // 7-day sweep
  const url = `${BASE_URL}/api/v1/board?from=${from.toISOString().slice(0, 10)}&to=${to.toISOString().slice(0, 10)}`;
  const res = http.get(url, { headers: { Authorization: `Bearer ${data.token}` } });
  check(res, {
    'availability 200': (r) => r.status === 200,
    'availability has occupancy': (r) => r.json('data.occupancy.length') > 0,
  });
}
