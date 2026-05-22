// Stream a host-side MCAP file through the gateway under the per-link netem
// stack so the operator can experience Foxglove under poor uplink conditions.
//
// Usage:
//   MCAP_HOST_PATH=/abs/path/to/heavy.mcap \
//   FOXGLOVE_API_URL=http://localhost:3000/api \
//   FOXGLOVE_DEVICE_TOKEN=fox_dt_... \
//   yarn stream-mcap
//
//   # Or pass the MCAP path positionally:
//   yarn stream-mcap /abs/path/to/heavy.mcap
//
// Prerequisites:
//   - The per-link stack from `yarn start-netem --perlink` is running, OR
//     this command will bring it up (with default impairment profiles).
//   - FOXGLOVE_API_URL and FOXGLOVE_DEVICE_TOKEN are set in the environment.
//   - The Foxglove app + web frontend are running on the host. See
//     rust/remote_access_tests/NETEM.md for the full setup.
//
// What it does:
//   1. Resolves the MCAP path to an absolute path and validates it.
//   2. Re-ups the per-link stack with MCAP_HOST_PATH exported so the file is
//      bind-mounted at /workspace/recording.mcap inside gateway-runner.
//   3. Builds `example_remote_access_stream_mcap` inside the container
//      (incremental after the first run, ~90s the first time).
//   4. Execs the streamer with the bind-mounted file.

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

interface Options {
  rustLog: string;
}

function compose(env: NodeJS.ProcessEnv, ...args: string[]): void {
  execFileSync("docker", ["compose", ...COMPOSE_FILES, ...args], {
    stdio: "inherit",
    env,
  });
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

function requireEnv(name: string): void {
  if (process.env[name] == null || process.env[name] === "") {
    console.error(
      `Error: ${name} is not set.\n` +
        "  Both FOXGLOVE_API_URL and FOXGLOVE_DEVICE_TOKEN are required; for example:\n" +
        "    export FOXGLOVE_API_URL=http://localhost:3000/api\n" +
        "    export FOXGLOVE_DEVICE_TOKEN=fox_dt_...",
    );
    process.exit(1);
  }
}

function run(opts: Options, positional: string | undefined): void {
  requireEnv("FOXGLOVE_API_URL");
  requireEnv("FOXGLOVE_DEVICE_TOKEN");
  const mcapPath = resolveMcapPath(positional);

  // Re-up the stack with the bind-mount pointing at the host file. Compose
  // expands ${MCAP_HOST_PATH} at this point; the gateway-runner restarts only
  // if the mount source actually changed.
  const upEnv: NodeJS.ProcessEnv = { ...process.env, MCAP_HOST_PATH: mcapPath };
  console.log(`Mounting ${mcapPath} -> /workspace/recording.mcap`);
  compose(upEnv, ...PROFILE_ARGS, "up", "-d", "--wait");

  // Build the streamer inside the container. Inherits stdio so the operator
  // sees cargo progress in real time.
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
    `FOXGLOVE_API_URL=${process.env.FOXGLOVE_API_URL ?? ""}`,
    "-e",
    `FOXGLOVE_DEVICE_TOKEN=${process.env.FOXGLOVE_DEVICE_TOKEN ?? ""}`,
    "-e",
    `RUST_LOG=${opts.rustLog}`,
    "gateway-runner",
    "/workspace/target-docker/release/example_remote_access_stream_mcap",
    "--file",
    "/workspace/recording.mcap",
  );
}

program
  .description("Stream a host-side MCAP file through the per-link netem stack's gateway-runner.")
  .argument("[mcap-path]", "Absolute path to an MCAP file (overrides MCAP_HOST_PATH)")
  .option("--rust-log <value>", "RUST_LOG value passed into the container", "foxglove=info,info")
  .action((positional: string | undefined, opts: Options) => {
    run(opts, positional);
  })
  .parse();
