// Set up the WireGuard viewer relay so a browser on the host can watch the
// per-link netem stack's stream with media traversing the impaired perlink
// path. See docker-compose.netem-relay.yml for the architecture and the relay
// runbook in rust/remote_access_tests/NETEM.md for the full flow.
//
// Usage:
//   yarn netem-relay         # set up keys/configs, start the relay, verify
//   yarn netem-relay down    # tear the relay down
//
// Prerequisites:
//   - wireguard-tools on the host (brew install wireguard-tools).
//   - The per-link stack is already running (`yarn stream-mcap` or
//     `yarn start-netem --perlink`). This script deliberately does not bring
//     that stack up itself: the stack is owned by those commands, and
//     recreating its services from here would re-derive their env from this
//     shell (see the stack-ownership note in scripts/streamMcap.ts).
//
// The one step this script cannot do for you is `sudo wg-quick up` — sudo
// needs a password prompt on the user's own terminal. The script prints the
// exact command and then waits, polling the relay container for the WireGuard
// handshake (container-side `wg show`; the host-side equivalent would itself
// need sudo).

import { program } from "commander";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import {
  buildHostConfig,
  buildRelayConfig,
  DEFAULT_TUNNEL_SUBNET,
  deriveTunnelAddressing,
  LIVEKIT_PERLINK_IP,
  PERLINK_SUBNET,
  RELAY_PERLINK_IP,
  TunnelAddressing,
} from "./netemRelayConfig";

const COMPOSE_FILES = [
  "-f",
  "docker-compose.yaml",
  "-f",
  "docker-compose.netem.yml",
  "-f",
  "docker-compose.netem-livekit.yml",
  "-f",
  "docker-compose.netem-relay.yml",
];
const PROFILE_ARGS = ["--profile", "relay"];

// Generated keys and configs live in a gitignored directory at the repo root.
// The host config's basename becomes the wg-quick interface name.
const RELAY_DIR = path.resolve(".netem-relay");
const RELAY_KEY_PATH = path.join(RELAY_DIR, "relay.key");
const HOST_KEY_PATH = path.join(RELAY_DIR, "host.key");
const RELAY_CONF_PATH = path.join(RELAY_DIR, "relay.conf");
const HOST_CONF_PATH = path.join(RELAY_DIR, "netem-relay.conf");

// A handshake older than this means the tunnel is not (or no longer) up —
// with PersistentKeepalive=25 a healthy tunnel re-handshakes every ~2 minutes.
const HANDSHAKE_FRESH_SECONDS = 180;
const HANDSHAKE_TIMEOUT_MS = 300_000;
const HANDSHAKE_POLL_MS = 2_000;

function compose(env: NodeJS.ProcessEnv, ...args: string[]): void {
  execFileSync("docker", ["compose", ...COMPOSE_FILES, ...args], {
    stdio: "inherit",
    env,
  });
}

// Like `compose`, but captures stdout instead of inheriting it. Used for
// queries (e.g. `ps -q`, `wg show`) whose output we need to inspect.
function composeCapture(env: NodeJS.ProcessEnv, ...args: string[]): string {
  return execFileSync("docker", ["compose", ...COMPOSE_FILES, ...args], {
    encoding: "utf8",
    env,
  });
}

function commandExists(name: string): boolean {
  try {
    execFileSync("which", [name], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function requireWireguardTools(): void {
  if (!commandExists("wg") || !commandExists("wg-quick")) {
    console.error(
      "Error: wireguard-tools is not installed on the host.\n" +
        "  Install it with: brew install wireguard-tools",
    );
    process.exit(1);
  }
}

function requirePerlinkStack(env: NodeJS.ProcessEnv): void {
  let livekitId = "";
  try {
    livekitId = composeCapture(env, "ps", "livekit", "-q").trim();
  } catch {
    // docker/compose unavailable or the query failed; treat as "not running".
  }
  if (livekitId === "") {
    console.error(
      "Error: the per-link stack isn't running.\n" +
        "  Start it with `yarn stream-mcap` or `yarn start-netem --perlink` first.\n" +
        "  (This script only manages the relay; it never recreates the stack.)",
    );
    process.exit(1);
  }
}

/** Load the private key at `file`, generating it on first run. */
function ensurePrivateKey(file: string): string {
  if (fs.existsSync(file)) {
    return fs.readFileSync(file, "utf8").trim();
  }
  const key = execFileSync("wg", ["genkey"], { encoding: "utf8" }).trim();
  fs.writeFileSync(file, key + "\n", { mode: 0o600 });
  console.log(`Generated ${path.basename(file)}`);
  return key;
}

function derivePublicKey(privateKey: string): string {
  return execFileSync("wg", ["pubkey"], { encoding: "utf8", input: privateKey }).trim();
}

/**
 * Write `content` to `file` unless it already matches. Returns true when the
 * file changed, which the caller uses to decide whether the relay container
 * must be recreated (it only reads the config at startup, and compose does not
 * watch bind-mounted file contents).
 */
function writeConfigIfChanged(file: string, content: string): boolean {
  let existing: string | undefined;
  try {
    existing = fs.readFileSync(file, "utf8");
  } catch {
    // First run, or the file was removed.
  }
  if (existing === content) {
    return false;
  }
  fs.writeFileSync(file, content, { mode: 0o600 });
  // writeFileSync's mode only applies on creation; enforce it on overwrite too
  // (wg-quick warns about configs readable by other users).
  fs.chmodSync(file, 0o600);
  return true;
}

function resolveTunnelAddressing(): TunnelAddressing {
  const subnet = process.env.NETEM_RELAY_TUNNEL_SUBNET ?? DEFAULT_TUNNEL_SUBNET;
  try {
    return deriveTunnelAddressing(subnet);
  } catch (err) {
    console.error(`Error: ${(err as Error).message}`);
    process.exit(1);
  }
}

/**
 * True once the relay reports a recent handshake with the host peer. Exec
 * failures (e.g. the container restarting) count as "not yet" — the preceding
 * `up --wait` already proved the container can become healthy.
 */
function handshakeIsFresh(env: NodeJS.ProcessEnv): boolean {
  let output: string;
  try {
    output = composeCapture(
      env,
      "exec",
      "-T",
      "viewer-relay",
      "wg",
      "show",
      "wg0",
      "latest-handshakes",
    );
  } catch {
    return false;
  }
  // One line per peer: "<public key>\t<unix epoch>" (0 = never).
  const epoch = Number(output.trim().split(/\s+/)[1] ?? "0");
  if (!Number.isFinite(epoch) || epoch === 0) {
    return false;
  }
  return Date.now() / 1000 - epoch < HANDSHAKE_FRESH_SECONDS;
}

async function waitForHandshake(env: NodeJS.ProcessEnv): Promise<boolean> {
  const deadline = Date.now() + HANDSHAKE_TIMEOUT_MS;
  let nextReminder = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (handshakeIsFresh(env)) {
      return true;
    }
    if (Date.now() >= nextReminder) {
      console.log(`Still waiting for the handshake — run: sudo wg-quick up ${HOST_CONF_PATH}`);
      nextReminder = Date.now() + 30_000;
    }
    await sleep(HANDSHAKE_POLL_MS);
  }
  return false;
}

/** Probe LiveKit's HTTP port through the tunnel; any response counts. */
async function livekitReachable(): Promise<boolean> {
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      await fetch(`http://${LIVEKIT_PERLINK_IP}:7880/`, { signal: AbortSignal.timeout(5_000) });
      return true;
    } catch {
      await sleep(2_000);
    }
  }
  return false;
}

/**
 * Warn when the LiveKit netem sidecar's viewer-download link is not pointed
 * at the relay IP — shaping would then miss the path the browser actually
 * uses. The sidecar always has the var (compose defaults it to empty).
 */
function checkViewerLinkDst(env: NodeJS.ProcessEnv): void {
  let dst = "";
  try {
    dst = composeCapture(env, "exec", "-T", "netem", "printenv", "NETEM_LINK_VIEWER_DST").trim();
  } catch {
    console.log(
      "Note: couldn't inspect the netem sidecar — viewer-download shaping may not target the relay.",
    );
    return;
  }
  if (dst !== RELAY_PERLINK_IP) {
    console.log("");
    console.log(
      `Warning: the netem sidecar's NETEM_LINK_VIEWER_DST is '${dst || "(unset)"}', not ${RELAY_PERLINK_IP}.`,
    );
    console.log("Viewer-download impairment will NOT apply to browser traffic through the relay.");
    console.log("To fix, restart the stack with the relay as the viewer link, e.g.:");
    console.log(`  NETEM_LINK_VIEWER_DST=${RELAY_PERLINK_IP} yarn stream-mcap ...`);
    console.log(`  NETEM_LINK_VIEWER_DST=${RELAY_PERLINK_IP} yarn start-netem --perlink`);
  }
}

async function up(): Promise<void> {
  requireWireguardTools();

  const addressing = resolveTunnelAddressing();
  // Pass the derived tunnel addressing to compose so the container and the
  // host config always agree, even under a NETEM_RELAY_TUNNEL_SUBNET override.
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    NETEM_RELAY_TUNNEL_ADDR: addressing.relayAddress,
    NETEM_RELAY_TUNNEL_SUBNET: addressing.subnet,
  };

  requirePerlinkStack(env);

  fs.mkdirSync(RELAY_DIR, { recursive: true, mode: 0o700 });
  const relayPrivateKey = ensurePrivateKey(RELAY_KEY_PATH);
  const hostPrivateKey = ensurePrivateKey(HOST_KEY_PATH);

  const relayChanged = writeConfigIfChanged(
    RELAY_CONF_PATH,
    buildRelayConfig({
      relayPrivateKey,
      hostPublicKey: derivePublicKey(hostPrivateKey),
      hostPeerAllowedIp: addressing.hostPeerAllowedIp,
    }),
  );
  const hostChanged = writeConfigIfChanged(
    HOST_CONF_PATH,
    buildHostConfig({
      hostPrivateKey,
      relayPublicKey: derivePublicKey(relayPrivateKey),
      hostAddress: addressing.hostAddress,
    }),
  );

  // --no-deps: never touch the stack this script doesn't own.
  // --force-recreate when the relay config changed: the container applies it
  // only at startup, and compose can't see bind-mounted content changes.
  const upArgs = [...PROFILE_ARGS, "up", "-d", "--no-deps", "--wait"];
  if (relayChanged) {
    upArgs.push("--force-recreate");
  }
  console.log("Starting the viewer-relay container...");
  try {
    compose(env, ...upArgs, "viewer-relay");
  } catch (err) {
    // compose inherits stderr, so its error is already the last thing on
    // screen — exit with the child's status instead of dumping a stack trace.
    process.exit((err as { status?: number }).status ?? 1);
  }

  console.log("");
  console.log("Relay is up. Bring up the host side of the tunnel (in your own terminal):");
  console.log("");
  console.log(`  sudo wg-quick up ${HOST_CONF_PATH}`);
  console.log("");
  console.log(`This adds exactly one route (${PERLINK_SUBNET} via the tunnel) and nothing else.`);
  if (hostChanged) {
    console.log("(The host config changed — if the tunnel is already up, wg-quick down/up it.)");
  }
  console.log("");
  console.log("Waiting for the WireGuard handshake...");

  if (!(await waitForHandshake(env))) {
    console.error("");
    console.error("Error: no WireGuard handshake within 5 minutes.");
    console.error(`  Did \`sudo wg-quick up ${HOST_CONF_PATH}\` succeed?`);
    console.error("  The relay container is still running; re-run `yarn netem-relay` to retry,");
    console.error("  or `yarn netem-relay down` to tear it down.");
    process.exit(1);
  }
  console.log("Handshake confirmed.");

  if (await livekitReachable()) {
    console.log(`LiveKit reachable through the tunnel (http://${LIVEKIT_PERLINK_IP}:7880).`);
  } else {
    console.error(
      `Error: handshake is up but http://${LIVEKIT_PERLINK_IP}:7880 is unreachable from the host.\n` +
        "  Check that the per-link stack is healthy (docker compose ps).",
    );
    process.exit(1);
  }

  checkViewerLinkDst(env);

  console.log("");
  console.log("Relay ready. Open the app and verify in chrome://webrtc-internals that the");
  console.log(`selected ICE candidate pair's remote address is on ${PERLINK_SUBNET} — otherwise`);
  console.log("media is bypassing the impaired path and netem settings won't apply.");
  console.log("");
  console.log("Tear down when done: yarn netem-relay down");
}

/** Best-effort: true if the host still routes the perlink subnet somewhere. */
function perlinkRoutePresent(): boolean {
  try {
    const routes = execFileSync("netstat", ["-rn"], { encoding: "utf8" });
    // BSD netstat compresses 10.99.0.0/24 to "10.99/24".
    return routes.split("\n").some((line) => /^10\.99[/.\s]/.test(line));
  } catch {
    return false;
  }
}

function down(): void {
  console.log("Stopping the viewer-relay container...");
  try {
    compose(process.env, ...PROFILE_ARGS, "rm", "-s", "-f", "viewer-relay");
  } catch {
    // Best-effort: the stack (or docker) may already be gone.
  }
  console.log("");
  if (perlinkRoutePresent()) {
    console.log(`The host tunnel still routes ${PERLINK_SUBNET}. Take it down with:`);
  } else {
    console.log("If the host tunnel is still up, take it down with:");
  }
  console.log("");
  console.log(`  sudo wg-quick down ${HOST_CONF_PATH}`);
  console.log("");
  console.log(`Keys and configs in ${RELAY_DIR} are kept for the next run.`);
}

program.description(
  "Set up the WireGuard viewer relay for watching the netem stack from a host browser.",
);

program
  .command("up", { isDefault: true })
  .description("Generate keys/configs, start the relay container, and verify the tunnel")
  .action(async () => {
    await up();
  });

program
  .command("down")
  .description("Stop the relay container and print the host-side teardown command")
  .action(() => {
    down();
  });

program.parseAsync().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
