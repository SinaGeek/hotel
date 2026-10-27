// Integration-test crate (README §16). No library code: everything lives in
// the tests/ directory. DB tests are gated on TEST_DATABASE_URL (e.g.
// postgres://postgres:postgres@localhost:5432/hms_test) and are skipped
// with a clear message when it is not set, so `cargo test` stays green
// without infrastructure.
