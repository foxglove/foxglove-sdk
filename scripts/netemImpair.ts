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
// Each invocation REPLACES all netem settings — unmentioned ones reset to
// default (e.g. dropping `loss` from the args clears it). `rate` is the
// exception: `netem_impair.py` appends an uncapped rate when none is given, so
// omitting it means "no rate limit" and `pristine` (`delay 0ms`) is unshaped.

import { program } from "commander";
import { execFileSync } from "node:child_process";

import { PROFILES, resolveArgs } from "./netemImpairArgs";

// Self-contained overlay; the base docker-compose.yaml isn't needed here.
const COMPOSE_FILES = ["-f", "docker-compose.netem-egress.yml"];

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

function run(opts: Options, trailing: string[]): void {
  let netemArgs: string[];
  try {
    netemArgs = resolveArgs(opts, trailing);
  } catch (err) {
    console.error(`Error: ${(err as Error).message}`);
    process.exit(1);
  }

  // Check the sidecar is up first, so we only blame "stack not running" when
  // that's the actual cause — not for e.g. rejected netem args, which fail in
  // the python script with their own error on stderr.
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
