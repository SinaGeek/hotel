// README Â§13 load scenario 3 â€” reservation save: unique room + far-future
// one-night stays (never collides with the seeded stays).
//
//   k6 run -e BASE_URL=http://localhost:8080 -e HOTEL=4443 \
//          -e USERNAME=loadmgr -e PASSWORD=... reservation_save.js
//
// Threshold (README Â§13): p95 < 200 ms
import http from 'k6/http';
import { check } from 'k6';
import exec from 'k6/execution';

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const HOTEL = parseInt(__ENV.HOTEL || '4443', 10);
const USERNAME = __ENV.USERNAME || 'loadmgr';
const PASSWORD = __ENV.PASSWORD;
const N = parseInt(__ENV.N || '200', 10);
// window start: run index (epoch minutes mod 700 * 10 days) â€” same semantics
// as the Rust runner; races between consecutive runs are mitigated by this.
const baseDay = 400 + (Math.floor(Date.now() / 60000) % 700) * 10;

export const options = {
  scenarios: {
    save: {
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
  const token = res.json('data.token');
  const headers = { Authorization: `Bearer ${token}` };

  const rooms = http.get(`${BASE_URL}/api/v1/rooms`, { headers }).json('data');
  check(rooms, { 'enough rooms': (r) => r.length >= 8 });
  const channels = http.get(`${BASE_URL}/api/v1/channels`, { headers }).json('data');
  const direct = channels.find((c) => c.kind === 'direct') || channels[0];

  // wall away base offset: rooms/3-day slots from the found base; the k6
  // scenario index gives the slot number so every iteration is unique.
  return { token, rooms, channelId: direct.id, baseDay };
}

export default function (data) {
  const i = exec.scenario.iterationInTest;   // 0..N-1, globally unique
  const room = data.rooms[i % data.rooms.length];
  const ci = data.baseDay + Math.floor(i / data.rooms.length) * 3;  // 1-night stays, 3-day slots
  const from = new Date(Date.now() + ci * 86400000);
  const to = new Date(from.getTime() + 1 * 86400000);
  const payload = JSON.stringify({
    room_id: room.id,
    channel_id: data.channelId,
    guest_id: null,
    checkin_date: from.toISOString().slice(0, 10),
    checkout_date: to.toISOString().slice(0, 10),
    adults: 1,
    children: 0,
  });
  const res = http.post(`${BASE_URL}/api/v1/reservations`, payload, {
    headers: { Authorization: `Bearer ${data.token}`, 'Content-Type': 'application/json' },
  });
  check(res, {
    'save 200': (r) => r.status === 200,
  });
}
