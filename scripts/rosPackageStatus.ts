// Report the current buildfarm status of our packages in each ROS 2 distro.
//
// For every (distro, package, target) it prints:
//   - the version released in rosdistro
//   - the version currently in the ros2-testing and main apt repos
//   - the state of the Jenkins binary job
//
// A row is "ok" when ros2-testing has the released version and the last binary
// build succeeded. The main repo is reported but does not gate the result:
// sync from testing to main lags, and rolling is often absent from main.
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
  binaryJobName,
  debName,
  jenkinsStatusFromApi,
  parseDistribution,
  parsePackagesIndex,
  rowOk,
  type Row,
} from "./rosPackageStatusReport";

// Sync copies ros2-testing -> main; both repos are reported.
const REPO_BASE = "https://repo.ros2.org";
const JENKINS_BASE = "https://build.ros2.org";
const ROSDISTRO_BASE = "https://raw.githubusercontent.com/ros/rosdistro/master";

const USER_AGENT = "foxglove-ros-package-status/1.0";

/** GET url; undefined on 404. */
async function get(url: string, timeoutMs: number): Promise<Buffer | undefined> {
  const res = await fetch(url, {
    headers: { "User-Agent": USER_AGENT },
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (res.status === 404) {
    return undefined;
  }
  if (!res.ok) {
    throw new Error(`GET ${url}: HTTP ${res.status}`);
  }
  return Buffer.from(await res.arrayBuffer());
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
const rosdistroVersions = memoize(async (distro: string): Promise<Record<string, string>> => {
  const raw = await get(`${ROSDISTRO_BASE}/${distro}/distribution.yaml`, 120_000);
  if (!raw) {
    return {};
  }
  return parseDistribution(yaml.load(raw.toString("utf8")));
});

/** debian package name -> Version, from <repo>/<os>/<component>/dists/<codename>/main/binary-<arch>/Packages.gz */
const packagesIndex = memoize(
  async (
    os: string,
    component: string,
    codename: string,
    arch: string,
  ): Promise<Record<string, string>> => {
    const raw = await get(
      `${REPO_BASE}/${os}/${component}/dists/${codename}/main/binary-${arch}/Packages.gz`,
      180_000,
    );
    if (!raw) {
      return {};
    }
    return parsePackagesIndex(gunzipSync(raw).toString("utf8"));
  },
);

/** One-word summary of a Jenkins job: missing / disabled / building / queued / never-built / <result>. */
const jenkinsStatus = memoize(async (job: string): Promise<string> => {
  const raw = await get(
    `${JENKINS_BASE}/job/${job}/api/json?tree=inQueue,disabled,lastBuild[result,building]`,
    60_000,
  );
  return jenkinsStatusFromApi(raw?.toString("utf8"));
});

async function rows(distros: string[], packages: string[]): Promise<Row[]> {
  const pending: Promise<Row>[] = [];
  for (const distro of distros) {
    const info = DISTROS[distro];
    if (info == undefined) {
      throw new Error(`unknown distro '${distro}'`);
    }
    for (const pkg of packages) {
      for (const target of info.targets) {
        const deb = debName(distro, pkg);
        const job = binaryJobName(distro, pkg, target);
        pending.push(
          (async (): Promise<Row> => {
            const [expected, testing, mainRepo, status] = await Promise.all([
              rosdistroVersions(distro),
              packagesIndex(target.os, "testing", target.codename, target.arch),
              packagesIndex(target.os, "main", target.codename, target.arch),
              jenkinsStatus(job),
            ]);
            return {
              distro,
              package: pkg,
              target: `${target.codename}/${target.arch}`,
              rosdistro: expected[pkg],
              testing: testing[deb],
              main: mainRepo[deb],
              jenkins: status,
              job_url: `${JENKINS_BASE}/job/${job}`,
            };
          })(),
        );
      }
    }
  }
  return await Promise.all(pending);
}

function printTable(rs: Row[]): void {
  const cols = ["distro", "package", "target", "rosdistro", "testing", "main", "jenkins"] as const;
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
  for (const distro of opts.distro ?? []) {
    if (!(distro in DISTROS)) {
      console.error(
        `Error: unknown distro '${distro}' (one of: ${Object.keys(DISTROS).join(", ")})`,
      );
      process.exit(2);
    }
  }
  const rs = await rows(opts.distro ?? Object.keys(DISTROS), opts.package ?? PACKAGES);
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
    console.error(`Error: ${(err as Error).message}`);
    process.exit(1);
  });
