-- SPEC-023 §Operational §4 (debt #94) — the server logs no statement's parameters: a
-- statement that is logged, by an error or by an operator who turns statement logging on,
-- is logged without them, so that a passphrase sent as a parameter (pgp_sym_encrypt's) never
-- reaches the server's log. Written once, at the initialisation, to postgresql.auto.conf.
ALTER SYSTEM SET log_statement = 'none';
ALTER SYSTEM SET log_min_duration_statement = -1;
ALTER SYSTEM SET log_parameter_max_length = 0;
ALTER SYSTEM SET log_parameter_max_length_on_error = 0;
SELECT pg_reload_conf();
