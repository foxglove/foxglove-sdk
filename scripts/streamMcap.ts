// Replay a host-side MCAP through a gateway program whose egress to the SFU is
// shaped by netem, to experience Foxglove under a constrained uplink. Uses
// docker-compose.netem-egress.yml (a `runner` + `runner-netem` sidecar); the
// full runbook is in rust/remote_access_tests/NETEM.md.
//
// Usage:
//   FOXGLOVE_DEVICE_TOKEN=fox_dt_... yarn stream-mcap /abs/path/to/heavy.mcap
//
//   FOXGLOVE_API_URL defaults to https://api.foxglove.dev; set it to target
//   another instance. MCAP_HOST_PATH is an alternative to the positional path.
//
// It bind-mounts the file at /data/recording.mcap, brings up the runner +
// sidecar, builds the streamer (slow on a cold cache — native WebRTC), and
// execs it. Set NETEM_EGRESS for a non-default starting profile, or retune live
// with `yarn netem-impair`. The stack is LEFT RUNNING on exit so retuning and
// re-runs stay fast; tear down with:
//   docker compose -f docker-compose.netem-egress.yml down

import { program } from "commander";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

// The egress overlay is self-contained — it does not need the base
// docker-compose.yaml (its only service, `livekit`, isn't used here; the runner
// reaches the SFU via FOXGLOVE_API_URL).
const COMPOSE_FILES = ["-f", "docker-compose.netem-egress.yml"];
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
  const e = err as { status?: number; signal?: string };
  // 130/143 are the conventional exit codes for SIGINT/SIGTERM, reported as
  // a plain status when an intermediary (e.g. docker exec) absorbs the signal.
  return e.status === 130 || e.status === 143 || e.signal === "SIGINT" || e.signal === "SIGTERM";
}

// Best-effort: kill any streamer still looping inside the runner. An
// interrupted or hard-killed run can leave one alive, holding the gateway lease
// so the next run fails with "another gateway holds the lease". A non-zero exit
// (nothing matched, or stack down) just means "nothing to clean up".
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
    console.error(`Error: ${name} is not set. Export it before running, e.g. ${name}=...`);
    process.exit(1);
  }
  return value;
}

function run(opts: Options, positional: string | undefined): void {
  const deviceToken = requireEnv("FOXGLOVE_DEVICE_TOKEN");
  // Optional: the gateway defaults to https://api.foxglove.dev when this is
  // unset (see Gateway's foxglove_api_url), so only forward it when provided.
  const apiUrl = process.env.FOXGLOVE_API_URL;
  const mcapPath = resolveMcapPath(positional);

  // Bring up (or refresh) the runner with the bind-mount pointing at the host
  // file. Compose expands ${MCAP_HOST_PATH} here; the runner is recreated if
  // any compose-visible config (including this mount source) changed, which
  // also restarts `runner-netem` and resets its qdisc to NETEM_EGRESS.
  const upEnv: NodeJS.ProcessEnv = { ...process.env, MCAP_HOST_PATH: mcapPath };

  // Without these handlers a Ctrl-C mid-`execFileSync` would kill this wrapper
  // and orphan the looping in-container streamer, which keeps holding the
  // gateway lease. In practice the child dies from its own copy of the signal
  // and the catch below cleans up; this body only runs for a signal landing
  // between compose calls. (SIGKILL skips all of this — the pre-launch
  // stopStreamer() covers that on the next run.)
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
    // the container's default environment. FOXGLOVE_API_URL is passed only when
    // set, letting the gateway fall back to its default otherwise.
    console.log("");
    console.log("Starting MCAP stream. Watch the device in your instance's web app.");
    console.log("Switch profiles mid-stream with: yarn netem-impair --profile <name>");
    console.log("");
    const execArgs = [
      "exec",
      "-e",
      `FOXGLOVE_DEVICE_TOKEN=${deviceToken}`,
      "-e",
      `RUST_LOG=${opts.rustLog}`,
    ];
    if (apiUrl != null && apiUrl !== "") {
      execArgs.push("-e", `FOXGLOVE_API_URL=${apiUrl}`);
    }
    execArgs.push("runner", STREAMER_BIN, "--file", "/data/recording.mcap");
    compose(upEnv, ...execArgs);
  } catch (err) {
    // This is also the Ctrl-C path: the signal hits the child first, so
    // execFileSync throws here while the wrapper's own copy is still latched.
    stopStreamer(upEnv);
    if (wasSignaled(err)) {
      console.log("\nStreamer stopped.");
      console.log(DOWN_HINT);
      const { status, signal } = err as { status?: number; signal?: string };
      process.exit(signal === "SIGTERM" || status === 143 ? 143 : 130);
    }
    // A real failure (build error, healthcheck timeout, streamer panic) already
    // printed to the inherited stderr; exit with the child's status so that
    // stays the last thing on screen rather than a node stack trace.
    console.error("\n" + DOWN_HINT);
    process.exit((err as { status?: number }).status ?? 1);
  }

  // The streamer loops forever, so reaching here means it exited on its own.
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
