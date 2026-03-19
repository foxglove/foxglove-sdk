// Start the netem stack for interactive testing with network impairment.
//
// Usage:
//   yarn start-netem                      # flat mode (single impairment)
//   yarn start-netem delay 200ms loss 5%  # flat mode, custom impairment
//   yarn start-netem --perlink            # per-link mode (bidirectional)
//
// Per-link mode starts gateway and viewer containers with independent
// impairment on each link. Override defaults with NETEM_GATEWAY_UPLOAD,
// NETEM_GATEWAY_DOWNLOAD, NETEM_VIEWER_UPLOAD, NETEM_VIEWER_DOWNLOAD.
// See rust/remote_access_tests/NETEM.md for details and scenarios.

import { program } from "commander";
import { execFileSync } from "node:child_process";

const COMPOSE_FILES = ["-f", "docker-compose.yaml", "-f", "docker-compose.netem.yml"];

const PERLINK_FILES = [...COMPOSE_FILES, "-f", "docker-compose.netem-livekit.yml"];

const PERLINK_DEFAULTS: Record<string, string> = {
  NETEM_GATEWAY_UPLOAD: "delay 30ms 10ms loss 2% rate 15mbit",
  NETEM_GATEWAY_DOWNLOAD: "delay 30ms 10ms loss 2% rate 100mbit",
  NETEM_VIEWER_UPLOAD: "delay 5ms rate 100mbit",
  NETEM_VIEWER_DOWNLOAD: "delay 5ms rate 500mbit",
};

function compose(files: string[], ...args: string[]): void {
  execFileSync("docker", ["compose", ...files, ...args], {
    stdio: "inherit",
    env: process.env,
  });
}

function composeDown(files: string[], profileArgs: string[]): void {
  try {
    compose(files, ...profileArgs, "down");
  } catch {
    // Best-effort cleanup on shutdown.
  }
}

function printTestCardHint(): void {
  console.log("");
  console.log("Run the test card in another terminal:");
  console.log("  FOXGLOVE_API_URL=http://localhost:3000/api \\");
  console.log("  FOXGLOVE_DEVICE_TOKEN=fox_dt_... \\");
  console.log("  cargo run -p example_remote_access --release");
  console.log("");
  console.log("Press Ctrl-C to stop.");
}

function waitForInterrupt(files: string[], profileArgs: string[]): void {
  const cleanup = () => {
    composeDown(files, profileArgs);
    process.exit(0);
  };
  process.on("SIGINT", cleanup);
  process.on("SIGTERM", cleanup);

  // Keep the event loop alive until a signal arrives. Without a referenced
  // handle, Node exits immediately after parse() returns.
  // eslint-disable-next-line @typescript-eslint/no-empty-function
  setInterval(() => {}, 2_147_483_647);
}

function startPerlink(): void {
  // LiveKit auto-detects its IP from the network interfaces (no --node-ip needed).
  process.env.NETEM_LINK_GATEWAY_DST = "10.99.0.31";
  process.env.NETEM_LINK_VIEWER_DST = "10.99.0.40";

  for (const [key, defaultValue] of Object.entries(PERLINK_DEFAULTS)) {
    process.env[key] ??= defaultValue;
  }

  compose(PERLINK_FILES, "--profile", "perlink", "up", "-d", "--wait");

  console.log("");
  console.log("Per-link netem is up.");
  console.log(`  Gateway upload:   ${process.env.NETEM_GATEWAY_UPLOAD ?? ""}`);
  console.log(`  Gateway download: ${process.env.NETEM_GATEWAY_DOWNLOAD ?? ""}`);
  console.log(`  Viewer upload:    ${process.env.NETEM_VIEWER_UPLOAD ?? ""}`);
  console.log(`  Viewer download:  ${process.env.NETEM_VIEWER_DOWNLOAD ?? ""}`);
  printTestCardHint();
  waitForInterrupt(PERLINK_FILES, ["--profile", "perlink"]);
}

function startFlat(netemArgs?: string): void {
  if (netemArgs != null) {
    process.env.NETEM_ARGS = netemArgs;
  }

  compose(COMPOSE_FILES, "up", "-d", "--wait");

  const activeArgs = process.env.NETEM_ARGS ?? "delay 80ms 20ms loss 2%";
  console.log("");
  console.log(`LiveKit + netem is up. NETEM_ARGS: ${activeArgs}`);
  printTestCardHint();
  waitForInterrupt(COMPOSE_FILES, []);
}

program
  .description("Start the netem stack for interactive testing with network impairment")
  .option("--perlink", "Per-link mode with bidirectional impairment")
  .argument("[netem-args...]", "Netem arguments for flat mode (e.g., delay 200ms loss 5%)")
  .action((netemArgs: string[], opts: { perlink: boolean }) => {
    if (opts.perlink && netemArgs.length > 0) {
      console.error(
        "Error: --perlink does not accept positional netem args.\n" +
          "Use NETEM_GATEWAY_UPLOAD/DOWNLOAD and NETEM_VIEWER_UPLOAD/DOWNLOAD env vars instead.\n" +
          "See rust/remote_access_tests/NETEM.md for details.",
      );
      process.exit(1);
    }
    if (opts.perlink) {
      startPerlink();
    } else {
      startFlat(netemArgs.length > 0 ? netemArgs.join(" ") : undefined);
    }
  })
  .parse();
