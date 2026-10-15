import { BlockList, isIP } from "node:net";

/**
 * SPEC-008's second Amendment 2026-10-15, *the source address behind a proxy*: the
 * address the per-IP counter counts and the audit rows record. Fastify's `trustProxy`
 * stays unset, so `req.ip` and the request log keep the peer's address (item 2).
 */

/** The hops `API_TRUSTED_PROXIES` lists (item 1). */
export type TrustedProxies = BlockList;

/**
 * An IP address in its one form (item 2): IPv4 as written, IPv6 in its canonical
 * form (RFC 5952, as the WHATWG URL parser writes a host), an IPv4-mapped IPv6
 * address as its IPv4 address. `null` for anything else: a name, an address with a
 * port, a zone index, surrounding spaces.
 */
export function canonicalIp(value: string): string | null {
  if (value.includes("%")) return null;
  const family = isIP(value);
  if (family === 4) return value;
  if (family !== 6) return null;
  const host = new URL(`http://[${value}]`).hostname.slice(1, -1);
  const mapped = /^::ffff:([0-9a-f]{1,4}):([0-9a-f]{1,4})$/.exec(host);
  if (mapped === null) return host;
  const high = Number.parseInt(mapped[1] as string, 16);
  const low = Number.parseInt(mapped[2] as string, 16);
  return `${high >> 8}.${high & 255}.${low >> 8}.${low & 255}`;
}

function familyOf(address: string): "ipv4" | "ipv6" {
  return isIP(address) === 4 ? "ipv4" : "ipv6";
}

/** Whether a range holds every IPv4 or every IPv6 address: a `/0`, or an IPv6 range over
 *  the whole IPv4-mapped space, would trust every peer of a family (item 1). */
function holdsEveryPeer(range: BlockList, family: "ipv4" | "ipv6"): boolean {
  if (range.check("0.0.0.0", "ipv4") && range.check("255.255.255.255", "ipv4")) return true;
  return (
    family === "ipv6" &&
    range.check("::", "ipv6") &&
    range.check("ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff", "ipv6")
  );
}

/**
 * Parse `API_TRUSTED_PROXIES` (item 1): comma-separated IPv4 and IPv6 addresses and
 * CIDR ranges; spaces around an entry and empty entries are ignored. Throws on any
 * other entry. `null` when the list is unset or holds no entry.
 */
export function parseTrustedProxies(value: string | undefined): TrustedProxies | null {
  const list = new BlockList();
  let entries = 0;
  for (const raw of (value ?? "").split(",")) {
    const entry = raw.trim();
    if (entry === "") continue;
    const slash = entry.indexOf("/");
    const address = slash === -1 ? entry : entry.slice(0, slash);
    if (address.includes("%") || isIP(address) === 0) {
      throw new Error(`not an IP address or a CIDR range: ${entry}`);
    }
    const family = familyOf(address);
    if (slash === -1) {
      list.addAddress(address, family);
    } else {
      const prefix = entry.slice(slash + 1);
      const max = family === "ipv4" ? 32 : 128;
      if (!/^\d{1,3}$/.test(prefix) || Number(prefix) > max) {
        throw new Error(`not a CIDR prefix length: ${entry}`);
      }
      const range = new BlockList();
      range.addSubnet(address, Number(prefix), family);
      if (holdsEveryPeer(range, family)) {
        throw new Error(`a range that trusts every peer: ${entry}`);
      }
      list.addSubnet(address, Number(prefix), family);
    }
    entries += 1;
  }
  return entries === 0 ? null : list;
}

function listed(trusted: TrustedProxies, address: string): boolean {
  return trusted.check(address, familyOf(address));
}

/**
 * The request's source address (item 2). From a listed peer, the `X-Forwarded-For`
 * entries are read from the right: the listed ones are passed and the first that is
 * not is taken; if every entry is listed, the leftmost. An entry that is not an IP
 * address, or no header, leaves the peer's address. From any other peer, the peer's
 * address. With no list, the peer's address as the socket gave it, the prior behavior;
 * so too when the socket gave none, as for a request whose connection has closed.
 */
export function sourceAddress(
  peer: string,
  forwardedFor: string | string[] | undefined,
  trusted: TrustedProxies | null,
): string {
  if (trusted === null || typeof peer !== "string") return peer;
  const peerAddress = canonicalIp(peer);
  if (peerAddress === null) return peer;
  if (!listed(trusted, peerAddress) || forwardedFor === undefined) return peerAddress;
  const header = Array.isArray(forwardedFor) ? forwardedFor.join(",") : forwardedFor;
  const entries = header.split(",").map((entry) => canonicalIp(entry.trim()));
  for (let i = entries.length - 1; i >= 0; i -= 1) {
    const entry = entries[i];
    if (entry === null || entry === undefined) return peerAddress;
    if (!listed(trusted, entry)) return entry;
  }
  return entries[0] ?? peerAddress;
}
