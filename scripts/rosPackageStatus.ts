// Report the current buildfarm status of our packages in each ROS 2 distro.
//
// For every (distro, package, target) it prints:
//   - the version released in rosdistro
//   - the version currently in the ros2-testing and main apt repos
//   - the state of the Jenkins binary job
//
// A row is "ok" when ros2-testing has the released version and the last binary
// build succeeded. The main repo is reported but does not gate the result:
// sync from testing to main lags, rolling is often absent from main, and a
// main-repo fetch failure is shown in `main_error` and does not fail the row.
//
// A failed fetch (timeout, DNS, TLS, or HTTP) does not abort the report.
// rosdistro, testing, and Jenkins failures are recorded in `error` with the
// URL, and the process exits non-zero. HTTP 404 stays "missing": the job or
// index is absent, not a failed request.
//
// Usage:
//   yarn ros-package-status [--distro humble ...] [--package foxglove_bridge ...] [--json]
//
// Edit PACKAGES / DISTROS in rosPackageStatusReport.ts to change what is checked.

import { program } from "commander";
import yaml from "js-yaml";
import { gunzipSync } from "node:zlib";

import {
  DISTROS,
  PACKAGES,
  errorText,
  fetchFailure,
  jenkinsStatusFromApi,
  parseDistribution,
  parsePackagesIndex,
  resolveSelection,
  rowOk,
  rows,
  type Loaded,
  type Row,
} from "./rosPackageStatusReport";

// Sync copies ros2-testing -> main; both repos are reported.
const REPO_BASE = "https://repo.ros2.org";
const JENKINS_BASE = "https://build.ros2.org";
const ROSDISTRO_BASE = "https://raw.githubusercontent.com/ros/rosdistro/master";

const USER_AGENT = "foxglove-ros-package-status/1.0";

/** GET url; undefined on 404. Network, TLS, and other HTTP failures name the URL. */
async function get(url: string, timeoutMs: number): Promise<Buffer | undefined> {
  let res: Response;
  try {
    res = await fetch(url, {
      headers: { "User-Agent": USER_AGENT },
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (err) {
    throw new Error(fetchFailure(url, err));
  }
  if (res.status === 404) {
    return undefined;
  }
  if (!res.ok) {
    throw new Error(`GET ${url}: HTTP ${res.status}`);
  }
  try {
    return Buffer.from(await res.arrayBuffer());
  } catch (err) {
    throw new Error(fetchFailure(url, err));
  }
}

/** Run `read` and return its failure, with the URL, instead of rejecting. A 404 returns `missing`. */
async function load<T>(
  url: string,
  timeoutMs: number,
  read: (raw: Buffer) => T,
  missing: T,
): Promise<Loaded<T>> {
  let raw: Buffer | undefined;
  try {
    raw = await get(url, timeoutMs);
  } catch (err) {
    return { error: fetchFailure(url, err) };
  }
  if (raw == undefined) {
    return { value: missing };
  }
  try {
    return { value: read(raw) };
  } catch (err) {
    return { error: fetchFailure(url, err) };
  }
}

/** Memoize an async function by the JSON encoding of its arguments. */
function memoize<A extends unknown[], T>(
  fn: (...args: A) => Promise<T>,
): (...args: A) => Promise<T> {
  const cache = new Map<string, Promise<T>>();
  return async (...args) => {
    const key = JSON.stringify(args);
    let pending = cache.get(key);
    if (!pending) {
      pending = fn(...args);
      cache.set(key, pending);
    }
    return await pending;
  };
}

/** package -> release version ('3.6.0-1'), or {} if the distro has no distribution.yaml. */
const rosdistroVersions = memoize(
  async (distro: string): Promise<Loaded<Record<string, string>>> => {
    const url = `${ROSDISTRO_BASE}/${distro}/distribution.yaml`;
    return await load(
      url,
      120_000,
      (raw) => parseDistribution(yaml.load(raw.toString("utf8"))),
      {},
    );
  },
);

/** debian package name -> Version, from <repo>/<os>/<component>/dists/<codename>/main/binary-<arch>/Packages.gz */
const packagesIndex = memoize(
  async (
    os: string,
    component: string,
    codename: string,
    arch: string,
  ): Promise<Loaded<Record<string, string>>> => {
    const url = `${REPO_BASE}/${os}/${component}/dists/${codename}/main/binary-${arch}/Packages.gz`;
    return await load(
      url,
      180_000,
      (raw) => parsePackagesIndex(gunzipSync(raw).toString("utf8")),
      {},
    );
  },
);

/** One-word summary of a Jenkins job: missing / disabled / building / queued / never-built / <result>. */
const jenkinsStatus = memoize(async (job: string): Promise<Loaded<string>> => {
  const url = `${JENKINS_BASE}/job/${job}/api/json?tree=inQueue,disabled,lastBuild[result,building]`;
  return await load(url, 60_000, (raw) => jenkinsStatusFromApi(raw.toString("utf8")), "missing");
});

function printTable(rs: Row[]): void {
  const cols = [
    "distro",
    "package",
    "target",
    "rosdistro",
    "testing",
    "main",
    "main_error",
    "jenkins",
    "error",
  ] as const;
  const vals = rs.map((r) => cols.map((c) => r[c] ?? "-"));
  const widths = cols.map((c, i) => Math.max(c.length, ...vals.map((v) => v[i]!.length)));
  const fmt = (v: readonly string[]) => v.map((s, i) => s.padEnd(widths[i]!)).join("  ");
  console.log("   " + fmt(cols));
  rs.forEach((r, i) => {
    console.log((rowOk(r) ? "ok " : "!! ") + fmt(vals[i]!));
  });
}

interface Options {
  distro?: string[];
  package?: string[];
  json?: boolean;
}

function collect(value: string, prev: string[] | undefined): string[] {
  return [...(prev ?? []), value];
}

async function main(opts: Options): Promise<void> {
  const selection = resolveSelection(opts.distro ?? Object.keys(DISTROS), opts.package ?? PACKAGES);
  const rs = await rows(
    selection.distros,
    selection.packages,
    { rosdistroVersions, packagesIndex, jenkinsStatus },
    JENKINS_BASE,
  );
  if (opts.json === true) {
    console.log(JSON.stringify(rs, undefined, 2));
  } else {
    printTable(rs);
  }
  process.exitCode = rs.every(rowOk) ? 0 : 1;
}

program
  .description("Report the buildfarm status of our packages in each ROS 2 distro.")
  .option(
    "-d, --distro <name>",
    `limit to this distro (repeatable; one of: ${Object.keys(DISTROS).join(", ")})`,
    collect,
  )
  .option("-p, --package <name>", "limit to this package (repeatable)", collect)
  .option("--json", "emit JSON instead of a table")
  .action(async (opts: Options) => {
    await main(opts);
  })
  .parseAsync()
  .catch((err: unknown) => {
    console.error(`Error: ${errorText(err)}`);
    process.exit(1);
  });
