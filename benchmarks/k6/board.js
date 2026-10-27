// README §13 load scenario 1 — room board (21-day window over a 60-room hotel).
// Parity harness for `cargo run -p hms-tests -r --example loadtest`.
//
//   k6 run -e BASE_URL=http://localhost:8080 -e HOTEL=4443 \
//          -e USERNAME=loadmgr -e PASSWORD=... board.js
//
// Threshold (README §13): p95 < 200 ms
import http from 'k6/http';
import { check } from 'k6';

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const HOTEL = __ENV.HOTEL || '4443';
const USERNAME = __ENV.USERNAME || 'loadmgr';
const PASSWORD = __ENV.PASSWORD;            // required; never hardcode
const N = parseInt(__ENV.N || '200', 10);   // total requests across VUs

export const options = {
  scenarios: {
    board: {
      executor: 'shared-iterations',
      vus: parseInt(__ENV.VUS || '16', 10),
      iterations: N,
      maxDuration: '5m',
    },
  },
  thresholds: {
    http_req_duration: ['p(95)<200'],
    // the login inside setup is not part of the target measurement
  },
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
  const token = res.json('data.token');
  if (!token) throw new Error('no token in login response');
  // warm-up: one board request so pool/plans are hot like production steady state
  http.get(`${BASE_URL}/api/v1/board?from=2026-10-27&to=2026-11-17`,
           { headers: { Authorization: `Bearer ${token}` } });
  return { token };
}

export default function (data) {
  const from = new Date();                       // today
  const to = new Date(from.getTime() + 21 * 86400000);
  const url = `${BASE_URL}/api/v1/board?from=${from.toISOString().slice(0, 10)}&to=${to.toISOString().slice(0, 10)}`;
  const res = http.get(url, { headers: { Authorization: `Bearer ${data.token}` } });
  check(res, {
    'board 200': (r) => r.status === 200,
    'board has rooms': (r) => r.json('data.rooms.length') > 0,
  });
}
