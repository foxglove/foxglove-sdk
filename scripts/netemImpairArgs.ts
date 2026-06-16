// Pure profile/argument resolution for `yarn netem-impair`, kept free of CLI
// and docker side effects so it is unit-testable. The CLI is in
// scripts/netemImpair.ts.

// Named profiles map to egress netem args. Presets mirror the scenarios in
// rust/remote_access_tests/NETEM.md; `severe` is tuned to saturate heavy-topic
// uplinks.
export const PROFILES: Record<string, string> = {
  pristine: "delay 0ms",
  starlink: "delay 30ms 10ms loss 2% rate 15mbit",
  "4g": "delay 50ms 15ms loss 3% rate 10mbit",
  "wifi-walls": "delay 15ms 10ms loss 8% rate 2mbit",
  severe: "delay 100ms 30ms loss 5% rate 2mbit",
};

export interface ResolveArgsInput {
  profile?: string;
}

/**
 * Resolve the netem args to apply, from a `--profile` name or raw trailing
 * args. Throws with an operator-facing message on invalid input; the CLI turns
 * that into an `Error: …` line and a non-zero exit.
 */
export function resolveArgs(opts: ResolveArgsInput, trailing: string[]): string[] {
  const hasTrailing = trailing.length > 0;
  if (opts.profile != null && hasTrailing) {
    throw new Error("pass either --profile or raw netem args, not both.");
  }
  const known = Object.keys(PROFILES).join(", ");
  if (opts.profile != null) {
    const preset = PROFILES[opts.profile];
    if (preset == null) {
      throw new Error(`unknown profile '${opts.profile}'. Known: ${known}`);
    }
    return preset.split(" ");
  }
  if (hasTrailing) {
    return trailing;
  }
  throw new Error(
    `nothing to apply.\n  Use --profile <name> (one of: ${known}), or\n` +
      "  pass raw netem args after `--`, e.g.: yarn netem-impair -- delay 500ms loss 10%",
  );
}
