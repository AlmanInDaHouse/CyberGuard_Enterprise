# CyberGuard proxy (SPEC-023 §Data contracts §2, §Operational §5): Caddy in front of the
# dashboard, with the product's Caddyfile. Built into cyberguard-proxy; the published bundle
# leaves this directory out.
#
# BUILD CONTEXT = repo ROOT (see deploy-ci.yml), for LICENSE/NOTICE.

FROM caddy:2.11.7-alpine@sha256:eb4d35b7452d1ab6171fbdbe8d9275b2301dad6dad92115d91e19fb9b24d510f
ARG CG_VERSION=dev
ARG CG_REVISION=unknown
LABEL org.opencontainers.image.source="https://github.com/AlmanInDaHouse/CyberGuard_Enterprise" \
      org.opencontainers.image.revision="${CG_REVISION}" \
      org.opencontainers.image.version="${CG_VERSION}" \
      org.opencontainers.image.licenses="Apache-2.0"
# Caddy's binary carries the file capability cap_net_bind_service, and a binary whose file
# capability lies outside the bounding set does not execute under cap_drop: [ALL]. The proxy
# listens on 8443 and 8080, which need none, so the capability is removed. /data and /config
# (Caddy's XDG directories: its internal CA, its configuration's autosave) are given to the
# proxy's user, so that new named volumes mounted there are writable by it. The uid is not
# 1000, which a host's first account usually has: the operator's TLS key, a host file, is
# given to it.
RUN setcap -r /usr/bin/caddy \
 && chown 10001:10001 /data /config
COPY deploy/docker/proxy/Caddyfile /etc/caddy/Caddyfile
COPY LICENSE NOTICE /usr/share/doc/cyberguard/
USER 10001:10001
EXPOSE 8443 8080
# The redirect on 8080 for the dashboard's hostname, answered by the proxy alone.
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --start-interval=2s --retries=3 \
  CMD ["sh", "-c", "printf 'GET / HTTP/1.0\\r\\nHost: %s\\r\\n\\r\\n' \"$CG_DASHBOARD_HOSTNAME\" | nc -w 3 127.0.0.1 8080 | head -n 1 | grep -q '^HTTP/1\\.[01] 308 '"]
CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
