# Dashboard TLS

In `provided` mode (`CG_TLS_MODE`), put here the certificate the dashboard is served with, as
`dashboard.crt` (PEM, the leaf first and its chain after it), and its private key, as
`dashboard.key`. `setup.sh` gives the key to the proxy's user (uid 10001) with mode `0400`.
The certificate names `CG_DASHBOARD_HOSTNAME` (SPEC-023 §Data contracts §3, §Operational §5).
