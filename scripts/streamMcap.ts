// Replay a host-side MCAP file through a gateway program whose egress to the
// SFU is shaped by netem, so the operator can experience Foxglove under a
// constrained uplink. Uses docker-compose.netem-egress.yml (a `runner` plus a
// `runner-netem` sidecar) — no per-link network or relay.
//
// Usage:
//   FOXGLOVE_API_URL=https://api.foxglove.dev \
//   FOXGLOVE_DEVICE_TOKEN=fox_dt_... \
//   yarn stream-mcap /abs/path/to/heavy.mcap
//
//   # MCAP_HOST_PATH is an alternative to the positional path.
//
// Prerequisites:
//   - FOXGLOVE_API_URL and FOXGLOVE_DEVICE_TOKEN set in the environment. A
//     deployed instance is the simplest target (browser playback works out of
//     the box); for a local SFU, run the app yourself and point the API URL at
//     it. See rust/remote_access_tests/NETEM.md.
//
// What it does:
//   1. Resolves and validates the MCAP path.
//   2. Brings up the runner + netem sidecar with the file bind-mounted at
//      /data/recording.mcap. Set NETEM_EGRESS for a non-default starting
//      profile, or retune live with `yarn netem-impair` once it's up.
//   3. Builds `example_remote_access_stream_mcap` in the container (the first
//      build from a cold cache is slow — it compiles native WebRTC code; later
//      builds are incremental via the persistent cargo volumes).
//   4. Execs the streamer with the bind-mounted file.
//
// The stack is LEFT RUNNING when the streamer exits, so `yarn netem-impair`
// keeps working and re-running is fast. Tear it down with:
//   docker compose -f docker-compose.yaml -f docker-compose.netem-egress.yml down

import { program } from "commander";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

const COMPOSE_FILES = ["-f", "docker-compose.yaml", "-f", "docker-compose.netem-egress.yml"];
const DOWN_HINT = `Tear down when done: docker compose ${COMPOSE_FILES.join(" ")} down`;
const STREAMER_BIN = "/workspace/target-docker/release/example_remote_access_stream_mcap";

interface Options {
  rustLog: string;
}

function compose(env: NodeJS.ProcessEnv, ...args: string[]): void {
  execFileSync("docker", ["compose", ...COMPOSE_FILES, ...args], {
    stdio: "inherit",
    env,
  });
}

/** True if the error from `execFileSync` means the child was interrupted (Ctrl-C or SIGTERM). */
function wasSignaled(err: unknown): boolean {
  const status = (err as { status?: number; signal?: string }).status;
  const signal = (err as { status?: number; signal?: string }).signal;
  // 130/143 are the conventional exit codes for SIGINT/SIGTERM, reported as
  // a plain status when an intermediary (e.g. docker exec) absorbs the signal.
  return status === 130 || status === 143 || signal === "SIGINT" || signal === "SIGTERM";
}

// Best-effort: stop any streamer still running inside the runner. The
// streamer runs via `docker compose exec` and loops MCAP playback forever, so an
// interrupted or hard-killed run can leave it alive — holding the gateway lease
// and making the next run fail with "another gateway holds the lease". `pkill`
// exits non-zero when nothing matches, and the exec fails if the stack is down;
// both mean "no orphan to clean up", so swallow the error.
function stopStreamer(env: NodeJS.ProcessEnv): void {
  try {
    execFileSync(
      "docker",
      ["compose", ...COMPOSE_FILES, "exec", "-T", "runner", "pkill", "-f", STREAMER_BIN],
      { stdio: "ignore", env },
    );
  } catch {
    // No matching process, or the stack isn't up — nothing to clean up.
  }
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
        "    export FOXGLOVE_API_URL=https://api.foxglove.dev\n" +
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

  // Bring up (or refresh) the runner with the bind-mount pointing at the host
  // file. Compose expands ${MCAP_HOST_PATH} here; the runner is recreated if
  // any compose-visible config (including this mount source) changed, which
  // also restarts `runner-netem` and resets its qdisc to NETEM_EGRESS.
  const upEnv: NodeJS.ProcessEnv = { ...process.env, MCAP_HOST_PATH: mcapPath };

  // Registering SIGINT/SIGTERM handlers replaces Node's default action (die
  // immediately), which would otherwise kill this wrapper mid-execFileSync and
  // orphan the in-container streamer — it keeps looping playback and holds the
  // gateway lease. The handler body itself almost never runs: while a compose
  // call is blocking, the wrapper's signal is only latched. The child dies
  // from its own copy of the signal, execFileSync throws, and the catch below
  // handles the signaled exit — its process.exit ends the process before the
  // latched signal is ever dispatched. The body only runs when a signal lands
  // in one of the brief gaps between compose calls, so it cleans up quietly
  // and exits with the conventional code. (A hard SIGKILL skips all of this,
  // but the pre-launch stopStreamer() below covers that on the next run.)
  const onSignal = (signal: NodeJS.Signals): void => {
    stopStreamer(upEnv);
    process.exit(signal === "SIGINT" ? 130 : 143);
  };
  process.on("SIGINT", onSignal);
  process.on("SIGTERM", onSignal);

  // The compose calls below inherit stdio, so their own errors print directly.
  // The try/catch keeps a container failure from burying that output under a
  // node stack trace.
  try {
    console.log(`Mounting ${mcapPath} -> /data/recording.mcap`);
    compose(upEnv, "up", "-d", "--wait", "runner", "runner-netem");

    console.log("");
    console.log("Building example_remote_access_stream_mcap inside the runner...");
    compose(
      upEnv,
      "exec",
      "runner",
      "cargo",
      "build",
      "-p",
      "example_remote_access_stream_mcap",
      "--release",
    );

    // Clear any streamer left over from an earlier run before claiming the
    // gateway lease — a lingering one (e.g. from a hard-killed run) would make
    // the watch stream fail with "another gateway holds the lease".
    stopStreamer(upEnv);

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
      "runner",
      STREAMER_BIN,
      "--file",
      "/data/recording.mcap",
    );
  } catch (err) {
    // Stop the streamer in case the exec died but left it running. This is
    // also the Ctrl-C path: the signal reaches the child first, execFileSync
    // throws, and this catch runs while the wrapper's own copy of the signal
    // is still latched (see the onSignal comment above).
    stopStreamer(upEnv);
    if (wasSignaled(err)) {
      console.log("\nStreamer stopped.");
      console.log(DOWN_HINT);
      const { status, signal } = err as { status?: number; signal?: string };
      process.exit(signal === "SIGTERM" || status === 143 ? 143 : 130);
    }
    // A non-signal failure (cargo build error, `up --wait` healthcheck
    // timeout, streamer panic) already printed its own error to the inherited
    // stderr. Exit with the child's status so that error stays the last thing
    // the operator sees, instead of throwing and dumping a node stack trace on
    // top of it.
    console.error("\n" + DOWN_HINT);
    process.exit((err as { status?: number }).status ?? 1);
  }

  // The streamer normally loops forever; reaching here means it exited on its
  // own, so make sure nothing lingers before pointing at teardown.
  stopStreamer(upEnv);
  console.log("");
  console.log(DOWN_HINT);
}

program
  .description("Replay a host-side MCAP file through a netem-shaped gateway egress.")
  .argument("[mcap-path]", "Absolute path to an MCAP file (overrides MCAP_HOST_PATH)")
  .option("--rust-log <value>", "RUST_LOG value passed into the container", "foxglove=debug,info")
  .action((positional: string | undefined, opts: Options) => {
    run(opts, positional);
  })
  .parse();
