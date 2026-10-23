-- SPEC-023 §Operational §4 (debt #94) — the server logs no statement's parameters: a
-- statement that is logged, by an error or by an operator who turns statement logging on,
-- is logged without them, so that a passphrase sent as a parameter (pgp_sym_encrypt's) does
-- not reach the server's log. A session can still set log_parameter_max_length_on_error
-- back, which any user may, and a superuser the other length. Written once, at the
-- initialisation, to postgresql.auto.conf, which the server reads when the entrypoint
-- restarts it.
ALTER SYSTEM SET log_statement = 'none';
ALTER SYSTEM SET log_min_duration_statement = -1;
ALTER SYSTEM SET log_parameter_max_length = 0;
ALTER SYSTEM SET log_parameter_max_length_on_error = 0;
