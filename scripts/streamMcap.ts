// Stream a host-side MCAP file through the gateway under the per-link netem
// stack so the operator can experience Foxglove under poor uplink conditions.
//
// Usage:
//   MCAP_HOST_PATH=/abs/path/to/heavy.mcap \
//   FOXGLOVE_API_URL=http://host.docker.internal:3000/api \
//   FOXGLOVE_DEVICE_TOKEN=fox_dt_... \
//   yarn stream-mcap
//
//   # Or pass the MCAP path positionally:
//   yarn stream-mcap /abs/path/to/heavy.mcap
//
// Prerequisites:
//   - FOXGLOVE_API_URL and FOXGLOVE_DEVICE_TOKEN are set in the environment.
//     The API URL must be reachable from inside `gateway-runner`, which only
//     joins the `perlink` network — use `http://host.docker.internal:3000/api`,
//     not `http://localhost:3000/api`.
//   - The Foxglove app + web frontend are running on the host. See
//     rust/remote_access_tests/NETEM.md for the full setup.
//
// What it does:
//   1. Resolves the MCAP path to an absolute path and validates it.
//   2. Brings up (or refreshes) the per-link stack with MCAP_HOST_PATH exported
//      so the file is bind-mounted at /data/recording.mcap inside
//      gateway-runner. This invocation owns the stack — `yarn start-netem
//      --perlink` from a separate terminal is NOT required, and any
//      NETEM_GATEWAY_UPLOAD set in that other terminal would be wiped here
//      when compose recreates the runner. To set a non-default starting
//      profile, export NETEM_GATEWAY_UPLOAD before invoking this command, or
//      use `yarn netem-impair --profile <name>` after the stack is up.
//   3. Builds `example_remote_access_stream_mcap` inside the container
//      (incremental after the first run, ~90s the first time).
//   4. Execs the streamer with the bind-mounted file.
//
// Unlike `yarn start-netem`, this script intentionally LEAVES THE STACK
// RUNNING when the streamer exits, so `yarn netem-impair` keeps working and
// re-running `yarn stream-mcap` is fast. Tear the stack down yourself when
// done:
//   docker compose -f docker-compose.yaml -f docker-compose.netem.yml \
//     -f docker-compose.netem-livekit.yml --profile perlink down

import { program } from "commander";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

const COMPOSE_FILES = [
  "-f",
  "docker-compose.yaml",
  "-f",
  "docker-compose.netem.yml",
  "-f",
  "docker-compose.netem-livekit.yml",
];
const PROFILE_ARGS = ["--profile", "perlink"];
const DOWN_HINT = `Tear down the stack when done: docker compose ${COMPOSE_FILES.join(
  " ",
)} --profile perlink down`;

interface Options {
  rustLog: string;
}

function compose(env: NodeJS.ProcessEnv, ...args: string[]): void {
  execFileSync("docker", ["compose", ...COMPOSE_FILES, ...args], {
    stdio: "inherit",
    env,
  });
}

/** True if the error from `execFileSync` means the child was interrupted (Ctrl-C). */
function wasSignaled(err: unknown): boolean {
  const status = (err as { status?: number; signal?: string }).status;
  const signal = (err as { status?: number; signal?: string }).signal;
  return status === 130 || signal === "SIGINT" || signal === "SIGTERM";
}

function resolveMcapPath(positional: string | undefined): string {
  const raw = positional ?? process.env.MCAP_HOST_PATH;
  if (raw == null || raw.length === 0) {
    console.error(
      "Error: no MCAP file provided.\n" +
        "  Set MCAP_HOST_PATH=/abs/path/to/file.mcap, or pass the path positionally:\n" +
        "    yarn stream-mcap /abs/path/to/file.mcap",
    );
    process.exit(1);
  }
  const abs = path.resolve(raw);
  // Compose splits bind-mount specs on `:` (host:container:options), so a `:`
  // anywhere in the resolved path silently corrupts the mount. `path.resolve`
  // can introduce a `:` via the cwd even when `raw` has none, so check `abs`.
  if (abs.includes(":")) {
    console.error(
      `Error: MCAP path must not contain ':' (compose treats ':' as a bind-mount separator): ${abs}`,
    );
    process.exit(1);
  }
  try {
    fs.accessSync(abs, fs.constants.R_OK);
  } catch {
    console.error(`Error: cannot read MCAP file at ${abs}`);
    process.exit(1);
  }
  const stat = fs.statSync(abs);
  if (!stat.isFile()) {
    console.error(`Error: ${abs} is not a regular file`);
    process.exit(1);
  }
  return abs;
}

function requireEnv(name: string): string {
  const value = process.env[name];
  if (value == null || value === "") {
    console.error(
      `Error: ${name} is not set.\n` +
        "  Both FOXGLOVE_API_URL and FOXGLOVE_DEVICE_TOKEN are required; for example:\n" +
        "    export FOXGLOVE_API_URL=http://host.docker.internal:3000/api\n" +
        "    export FOXGLOVE_DEVICE_TOKEN=fox_dt_...",
    );
    process.exit(1);
  }
  return value;
}

function run(opts: Options, positional: string | undefined): void {
  const apiUrl = requireEnv("FOXGLOVE_API_URL");
  const deviceToken = requireEnv("FOXGLOVE_DEVICE_TOKEN");
  const mcapPath = resolveMcapPath(positional);

  // Bring up (or refresh) the stack with the bind-mount pointing at the host
  // file. Compose expands ${MCAP_HOST_PATH} here; gateway-runner is recreated
  // if any compose-visible config (including this mount source) changed since
  // the last `up`. That recreation also restarts `gateway-netem`, resetting
  // its qdisc to NETEM_GATEWAY_UPLOAD's value in *this* env.
  const upEnv: NodeJS.ProcessEnv = { ...process.env, MCAP_HOST_PATH: mcapPath };

  // The compose calls below inherit stdio, so their own errors print directly.
  // A single try/catch keeps a Ctrl-C (or container failure) from burying that
  // output under a node stack trace.
  try {
    console.log(`Mounting ${mcapPath} -> /data/recording.mcap`);
    compose(upEnv, ...PROFILE_ARGS, "up", "-d", "--wait");

    console.log("");
    console.log("Building example_remote_access_stream_mcap inside gateway-runner...");
    compose(
      upEnv,
      "exec",
      "gateway-runner",
      "cargo",
      "build",
      "-p",
      "example_remote_access_stream_mcap",
      "--release",
    );

    // Forward only the env vars the streamer needs; everything else stays in
    // the container's default environment.
    console.log("");
    console.log("Starting MCAP stream. Open http://localhost:8080 to view.");
    console.log("Switch profiles mid-stream with: yarn netem-impair --profile <name>");
    console.log("");
    compose(
      upEnv,
      "exec",
      "-e",
      `FOXGLOVE_API_URL=${apiUrl}`,
      "-e",
      `FOXGLOVE_DEVICE_TOKEN=${deviceToken}`,
      "-e",
      `RUST_LOG=${opts.rustLog}`,
      "gateway-runner",
      "/workspace/target-docker/release/example_remote_access_stream_mcap",
      "--file",
      "/data/recording.mcap",
    );
  } catch (err) {
    if (wasSignaled(err)) {
      console.log("\nStreamer stopped.");
      console.log(DOWN_HINT);
      return;
    }
    // A non-signal failure (cargo build error, `up --wait` healthcheck
    // timeout, streamer panic) already printed its own error to the inherited
    // stderr. Exit with the child's status so that error stays the last thing
    // the operator sees, instead of throwing and dumping a node stack trace on
    // top of it.
    console.error("\n" + DOWN_HINT);
    process.exit((err as { status?: number }).status ?? 1);
  }

  console.log("");
  console.log(DOWN_HINT);
}

program
  .description("Stream a host-side MCAP file through the per-link netem stack's gateway-runner.")
  .argument("[mcap-path]", "Absolute path to an MCAP file (overrides MCAP_HOST_PATH)")
  .option("--rust-log <value>", "RUST_LOG value passed into the container", "foxglove=debug,info")
  .action((positional: string | undefined, opts: Options) => {
    run(opts, positional);
  })
  .parse();
