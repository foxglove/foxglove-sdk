// Live-update the gateway-upload netem impairment without restarting the
// per-link stack. Targets the `gateway-netem` sidecar, which shapes the
// gateway-runner's egress to LiveKit (the "uplink" in the FLE-372 scenario).
//
// Usage:
//   yarn netem-impair --profile starlink
//   yarn netem-impair --profile severe
//   yarn netem-impair --profile pristine
//   yarn netem-impair -- delay 500ms loss 10%       # raw netem args
//
// Each invocation REPLACES all netem settings on the qdisc. Unmentioned
// settings reset to 0 — for example, switching from "delay 500ms loss 20%"
// to "delay 400ms" resets loss to 0%.
//
// Scope: this wrapper hardcodes the `gateway-netem` sidecar, so it only
// retunes the gateway-upload link. The underlying `netem_impair.py` is not so
// limited — exec'd into the LiveKit-side `netem` sidecar it updates all
// download links at once (see "Changing impairment live" in
// rust/remote_access_tests/NETEM.md). Per-link viewer/download retuning still
// requires a stack restart with new NETEM_* env vars.

import { program } from "commander";
import { execFileSync } from "node:child_process";

const COMPOSE_FILES = [
  "-f",
  "docker-compose.yaml",
  "-f",
  "docker-compose.netem.yml",
  "-f",
  "docker-compose.netem-livekit.yml",
];

// Named profiles map to gateway-upload netem args. Presets mirror the
// scenarios documented in rust/remote_access_tests/NETEM.md; `severe` is
// tuned to saturate heavy-topic uplinks (FLE-372).
const PROFILES: Record<string, string> = {
  pristine: "delay 0ms",
  starlink: "delay 30ms 10ms loss 2% rate 15mbit",
  "4g": "delay 50ms 15ms loss 3% rate 10mbit",
  "wifi-walls": "delay 15ms 10ms loss 8% rate 2mbit",
  severe: "delay 100ms 30ms loss 5% rate 2mbit",
};

interface Options {
  profile?: string;
}

function compose(...args: string[]): void {
  execFileSync("docker", ["compose", ...COMPOSE_FILES, ...args], {
    stdio: "inherit",
    env: process.env,
  });
}

function resolveArgs(opts: Options, trailing: string[]): string[] {
  const hasProfile = opts.profile != null;
  const hasTrailing = trailing.length > 0;
  if (hasProfile && hasTrailing) {
    console.error("Error: pass either --profile or raw netem args, not both.");
    process.exit(1);
  }
  if (hasProfile) {
    const preset = PROFILES[opts.profile!];
    if (preset == null) {
      const known = Object.keys(PROFILES).join(", ");
      console.error(`Error: unknown profile '${opts.profile!}'. Known: ${known}`);
      process.exit(1);
    }
    return preset.split(" ");
  }
  if (hasTrailing) {
    return trailing;
  }
  console.error(
    "Error: nothing to apply.\n" +
      `  Use --profile <name> (one of: ${Object.keys(PROFILES).join(", ")}), or\n` +
      "  pass raw netem args after `--`, e.g.: yarn netem-impair -- delay 500ms loss 10%",
  );
  process.exit(1);
}

function run(opts: Options, trailing: string[]): void {
  const netemArgs = resolveArgs(opts, trailing);
  console.log(`gateway upload: netem ${netemArgs.join(" ")}`);
  try {
    compose("exec", "gateway-netem", "python3", "/netem_impair.py", ...netemArgs);
  } catch {
    // The most common cause is that the per-link stack isn't running, which
    // produces an opaque docker error. Point the operator at the fix instead
    // of dumping a node stack trace.
    console.error(
      "\nError: could not reach the gateway-netem sidecar.\n" +
        "  Is the per-link stack running? Start it with `yarn stream-mcap` or\n" +
        "  `yarn start-netem --perlink` first.",
    );
    process.exit(1);
  }
}

program
  .description("Live-update the gateway-upload netem impairment on the running per-link stack.")
  .option("-p, --profile <name>", `Named profile (one of: ${Object.keys(PROFILES).join(", ")})`)
  .argument("[netem-args...]", "Raw netem args (after `--`)")
  .action((trailing: string[], opts: Options) => {
    run(opts, trailing);
  })
  .parse();
