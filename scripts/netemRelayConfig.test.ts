// Tests for the pure netem viewer-relay helpers: tunnel-subnet derivation,
// key validation, and WireGuard config templating — in particular the
// AllowedIPs scoping invariant that keeps host traffic off the tunnel.

import {
  buildHostConfig,
  buildRelayConfig,
  DEFAULT_TUNNEL_SUBNET,
  deriveTunnelAddressing,
  PERLINK_SUBNET,
  WG_LISTEN_PORT,
} from "./netemRelayConfig";

// Structurally valid WireGuard keys (32 bytes base64); content is arbitrary.
const RELAY_PRIVATE_KEY = "a".repeat(43) + "=";
const RELAY_PUBLIC_KEY = "b".repeat(43) + "=";
const HOST_PRIVATE_KEY = "c".repeat(43) + "=";
const HOST_PUBLIC_KEY = "d".repeat(43) + "=";

describe("deriveTunnelAddressing", () => {
  it("derives relay (.1) and host (.2) addresses from the default subnet", () => {
    expect(deriveTunnelAddressing(DEFAULT_TUNNEL_SUBNET)).toEqual({
      subnet: "10.200.0.0/24",
      relayAddress: "10.200.0.1/24",
      hostAddress: "10.200.0.2/24",
      hostPeerAllowedIp: "10.200.0.2/32",
    });
  });

  it("accepts a custom /24 subnet", () => {
    expect(deriveTunnelAddressing("192.168.77.0/24").hostAddress).toBe("192.168.77.2/24");
  });

  it.each(["10.200.0.0/16", "10.200.0.5/24", "10.200.0/24", "10.200.0.0", "bogus"])(
    "rejects malformed subnet %s",
    (subnet) => {
      expect(() => deriveTunnelAddressing(subnet)).toThrow(/Invalid tunnel subnet/);
    },
  );

  it("rejects octets out of range", () => {
    expect(() => deriveTunnelAddressing("10.300.0.0/24")).toThrow(/octet out of range/);
  });

  it.each(["10.99.0.0/24", "10.98.0.0/24"])(
    "rejects %s, which collides with the stack's own networks",
    (subnet) => {
      expect(() => deriveTunnelAddressing(subnet)).toThrow(/collides/);
    },
  );
});

describe("buildRelayConfig", () => {
  const config = buildRelayConfig({
    relayPrivateKey: RELAY_PRIVATE_KEY,
    hostPublicKey: HOST_PUBLIC_KEY,
    hostPeerAllowedIp: "10.200.0.2/32",
  });

  it("listens on the published WireGuard port", () => {
    expect(config).toContain(`ListenPort = ${WG_LISTEN_PORT}`);
  });

  it("embeds the relay private key and host public key", () => {
    expect(config).toContain(`PrivateKey = ${RELAY_PRIVATE_KEY}`);
    expect(config).toContain(`PublicKey = ${HOST_PUBLIC_KEY}`);
  });

  it("restricts the host peer to its /32 tunnel address", () => {
    expect(config).toContain("AllowedIPs = 10.200.0.2/32");
  });

  it("omits wg-quick-only keys, since it is applied via `wg setconf`", () => {
    expect(config).not.toMatch(/^Address/m);
    expect(config).not.toMatch(/^MTU/m);
  });

  it("rejects values that are not WireGuard keys", () => {
    expect(() =>
      buildRelayConfig({
        relayPrivateKey: "/path/to/relay.key",
        hostPublicKey: HOST_PUBLIC_KEY,
        hostPeerAllowedIp: "10.200.0.2/32",
      }),
    ).toThrow(/relay private key/);
  });
});

describe("buildHostConfig", () => {
  const config = buildHostConfig({
    hostPrivateKey: HOST_PRIVATE_KEY,
    relayPublicKey: RELAY_PUBLIC_KEY,
    hostAddress: "10.200.0.2/24",
  });

  it("routes exactly the perlink subnet through the tunnel and nothing else", () => {
    const allowedIps = config
      .split("\n")
      .filter((line) => line.startsWith("AllowedIPs"))
      .map((line) => line.split("=")[1]?.trim());
    expect(allowedIps).toEqual([PERLINK_SUBNET]);
    // A default route (or any wider scope) would let ICE bypass the impaired
    // path — the whole point of the scoping.
    expect(config).not.toContain("0.0.0.0/0");
  });

  it("embeds the host private key, relay public key, and tunnel address", () => {
    expect(config).toContain(`PrivateKey = ${HOST_PRIVATE_KEY}`);
    expect(config).toContain(`PublicKey = ${RELAY_PUBLIC_KEY}`);
    expect(config).toContain("Address = 10.200.0.2/24");
  });

  it("targets the published port on localhost and keeps the mapping alive", () => {
    expect(config).toContain(`Endpoint = 127.0.0.1:${WG_LISTEN_PORT}`);
    expect(config).toContain("PersistentKeepalive = 25");
  });

  it("rejects values that are not WireGuard keys", () => {
    expect(() =>
      buildHostConfig({
        hostPrivateKey: HOST_PRIVATE_KEY,
        relayPublicKey: "",
        hostAddress: "10.200.0.2/24",
      }),
    ).toThrow(/relay public key/);
  });
});
