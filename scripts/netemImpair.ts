// Live-update the egress netem impairment on the running netem-egress stack
// without restarting it. Targets the `runner-netem` sidecar, which shapes the
// runner's egress to the SFU (the "uplink").
//
// Usage:
//   yarn netem-impair --profile starlink
//   yarn netem-impair --profile severe
//   yarn netem-impair --profile pristine
//   yarn netem-impair -- delay 500ms loss 10%       # raw netem args
//
// Each invocation REPLACES all netem settings on the qdisc — unmentioned
// settings reset to their defaults. For example, switching from
// "delay 500ms loss 20%" to "delay 400ms" resets loss to 0%. `rate` is a kernel
// special case (it persists across a bare `tc qdisc change`), so `netem_impair.py`
// appends an uncapped rate when none is given; omitting `rate` here therefore
// means "no rate limit", consistent with the other settings. So `pristine`
// (`delay 0ms`) really does restore an unshaped link.

import { program } from "commander";
import { execFileSync } from "node:child_process";

const COMPOSE_FILES = ["-f", "docker-compose.yaml", "-f", "docker-compose.netem-egress.yml"];

// Named profiles map to egress netem args. Presets mirror the scenarios
// documented in rust/remote_access_tests/NETEM.md; `severe` is tuned to
// saturate heavy-topic uplinks.
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

// Like `compose`, but captures stdout instead of inheriting it. Used for
// queries (e.g. `ps -q`) whose output we need to inspect.
function composeCapture(...args: string[]): string {
  return execFileSync("docker", ["compose", ...COMPOSE_FILES, ...args], {
    encoding: "utf8",
    env: process.env,
  });
}

function resolveArgs(opts: Options, trailing: string[]): string[] {
  const hasTrailing = trailing.length > 0;
  if (opts.profile != null && hasTrailing) {
    console.error("Error: pass either --profile or raw netem args, not both.");
    process.exit(1);
  }
  if (opts.profile != null) {
    const preset = PROFILES[opts.profile];
    if (preset == null) {
      const known = Object.keys(PROFILES).join(", ");
      console.error(`Error: unknown profile '${opts.profile}'. Known: ${known}`);
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

  // Check the sidecar is up front. `ps -q` prints its container ID when
  // running and nothing otherwise, so we can give the "stack not running"
  // hint only when that's actually the cause — rather than blaming the stack
  // for every failure (e.g. rejected netem args, which exit non-zero from the
  // python script with its own error already on stderr).
  let sidecarId = "";
  try {
    sidecarId = composeCapture("ps", "runner-netem", "-q").trim();
  } catch {
    // docker/compose unavailable or the query failed; treat as "not running".
  }
  if (sidecarId === "") {
    console.error(
      "Error: the runner-netem sidecar isn't running.\n" +
        "  Start it with `yarn stream-mcap` first.",
    );
    process.exit(1);
  }

  console.log(`egress: netem ${netemArgs.join(" ")}`);
  try {
    compose("exec", "runner-netem", "python3", "/netem_impair.py", ...netemArgs);
  } catch (err) {
    // The sidecar is running, so the failure came from netem_impair.py itself
    // (most likely rejected args). Its stderr is already inherited, so just
    // exit with its status instead of dumping a node stack trace on top.
    process.exit((err as { status?: number }).status ?? 1);
  }
}

program
  .description("Live-update the egress netem impairment on the running netem-egress stack.")
  .option("-p, --profile <name>", `Named profile (one of: ${Object.keys(PROFILES).join(", ")})`)
  .argument("[netem-args...]", "Raw netem args (after `--`)")
  .action((trailing: string[], opts: Options) => {
    run(opts, trailing);
  })
  .parse();
