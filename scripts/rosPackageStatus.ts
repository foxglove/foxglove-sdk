// Report the current buildfarm status of our packages in each ROS 2 distro.
//
// For every (distro, package, target) it prints:
//   - the version released in rosdistro
//   - the version currently in the ros2-testing and main apt repos
//   - the state of the Jenkins binary job
//
// Usage:
//   yarn ros-package-status [--distro humble ...] [--package foxglove_bridge ...] [--json]
//
// Edit PACKAGES / DISTROS below to change what is checked.

import { program } from "commander";
import yaml from "js-yaml";
import { gunzipSync } from "node:zlib";

// Packages we maintain. Must match rosdistro package names.
const PACKAGES = ["foxglove_bridge", "foxglove_msgs"];

interface Target {
  os: string;
  codename: string;
  arch: string;
  // The token after "bin_" in the Jenkins job name, e.g.
  //   Kbin_uN64__foxglove_bridge__ubuntu_noble_amd64__binary
  // It is irregular across targets, so list it explicitly. To see all jobs:
  //   curl -sg 'https://build.ros2.org/api/json?tree=jobs[name]'
  jenkins: string;
}

interface Distro {
  // First letter of buildfarm job names.
  prefix: string;
  targets: Target[];
}

const DISTROS: Record<string, Distro> = {
  humble: {
    prefix: "H",
    targets: [
      { os: "ubuntu", codename: "jammy", arch: "amd64", jenkins: "uJ64" },
      { os: "ubuntu", codename: "jammy", arch: "arm64", jenkins: "ujv8_uJv8" },
    ],
  },
  jazzy: {
    prefix: "J",
    targets: [
      { os: "ubuntu", codename: "noble", arch: "amd64", jenkins: "uN64" },
      { os: "ubuntu", codename: "noble", arch: "arm64", jenkins: "unv8_uNv8" },
    ],
  },
  kilted: {
    prefix: "K",
    targets: [
      { os: "ubuntu", codename: "noble", arch: "amd64", jenkins: "uN64" },
      { os: "ubuntu", codename: "noble", arch: "arm64", jenkins: "unv8_uNv8" },
    ],
  },
  lyrical: {
    prefix: "L",
    targets: [
      { os: "ubuntu", codename: "resolute", arch: "amd64", jenkins: "uR64" },
      { os: "ubuntu", codename: "resolute", arch: "arm64", jenkins: "armv8_uRv8" },
    ],
  },
  rolling: {
    prefix: "R",
    targets: [
      { os: "ubuntu", codename: "resolute", arch: "amd64", jenkins: "uR64" },
      { os: "ubuntu", codename: "resolute", arch: "arm64", jenkins: "unv8_uRv8" },
    ],
  },
};

// Sync copies ros2-testing -> main; both repos are reported.
const REPO_BASE = "https://repo.ros2.org";
const JENKINS_BASE = "https://build.ros2.org";
const ROSDISTRO_BASE = "https://raw.githubusercontent.com/ros/rosdistro/master";

const USER_AGENT = "foxglove-ros-package-status/1.0";

interface Row {
  distro: string;
  package: string;
  target: string;
  rosdistro: string | undefined;
  testing: string | undefined;
  main: string | undefined;
  jenkins: string;
  job_url: string;
}

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
    let p = cache.get(key);
    if (!p) {
      p = fn(...args);
      cache.set(key, p);
    }
    return await p;
  };
}

interface DistributionYaml {
  repositories?: Record<string, { release?: { version?: string; packages?: string[] } }>;
}

/** package -> release version ('3.5.0-1'), or {} if the distro has no distribution.yaml. */
const rosdistroVersions = memoize(async (distro: string): Promise<Record<string, string>> => {
  const raw = await get(`${ROSDISTRO_BASE}/${distro}/distribution.yaml`, 120_000);
  if (!raw) {
    return {};
  }
  const doc = yaml.load(raw.toString("utf8")) as DistributionYaml;
  const out: Record<string, string> = {};
  for (const [repoName, repo] of Object.entries(doc.repositories ?? {})) {
    const version = repo.release?.version;
    if (version == undefined) {
      continue;
    }
    for (const pkg of repo.release?.packages ?? [repoName]) {
      out[pkg] = version;
    }
  }
  return out;
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
    const out: Record<string, string> = {};
    let name: string | undefined;
    for (const line of gunzipSync(raw).toString("utf8").split("\n")) {
      if (line.startsWith("Package: ")) {
        name = line.slice(9).trim();
      } else if (line.startsWith("Version: ") && name != undefined) {
        out[name] = line.slice(9).trim();
      } else if (line.trim() === "") {
        name = undefined;
      }
    }
    return out;
  },
);

function debName(distro: string, pkg: string): string {
  return `ros-${distro}-${pkg.replace(/_/g, "-")}`;
}

interface JenkinsJob {
  inQueue?: boolean;
  disabled?: boolean;
  lastBuild?: { result?: string | null; building?: boolean } | null;
}

/** One-word summary of a Jenkins job: missing / disabled / building / queued / never-built / <result>. */
const jenkinsStatus = memoize(async (job: string): Promise<string> => {
  const raw = await get(
    `${JENKINS_BASE}/job/${job}/api/json?tree=inQueue,disabled,lastBuild[result,building]`,
    60_000,
  );
  if (!raw) {
    return "missing";
  }
  const j = JSON.parse(raw.toString("utf8")) as JenkinsJob;
  const lb = j.lastBuild ?? {};
  if (j.disabled === true) {
    return "disabled";
  }
  if (lb.building === true) {
    return "building";
  }
  if (j.inQueue === true) {
    return "queued";
  }
  return (lb.result ?? "never-built").toLowerCase();
});

async function rows(distros: string[], packages: string[]): Promise<Row[]> {
  const pending: Promise<Row>[] = [];
  for (const distro of distros) {
    const { prefix, targets } = DISTROS[distro]!;
    for (const pkg of packages) {
      for (const { os, codename, arch, jenkins } of targets) {
        const deb = debName(distro, pkg);
        const job = `${prefix}bin_${jenkins}__${pkg}__${os}_${codename}_${arch}__binary`;
        pending.push(
          (async (): Promise<Row> => {
            const [expected, testing, mainRepo, status] = await Promise.all([
              rosdistroVersions(distro),
              packagesIndex(os, "testing", codename, arch),
              packagesIndex(os, "main", codename, arch),
              jenkinsStatus(job),
            ]);
            return {
              distro,
              package: pkg,
              target: `${codename}/${arch}`,
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

/** Testing has the released version and the last build succeeded. */
function ok(row: Row): boolean {
  return (
    row.rosdistro != undefined &&
    row.testing != undefined &&
    row.testing.startsWith(row.rosdistro) &&
    row.jenkins === "success"
  );
}

function printTable(rs: Row[]): void {
  const cols = ["distro", "package", "target", "rosdistro", "testing", "main", "jenkins"] as const;
  const vals = rs.map((r) => cols.map((c) => r[c] ?? "-"));
  const widths = cols.map((c, i) => Math.max(c.length, ...vals.map((v) => v[i]!.length)));
  const fmt = (v: readonly string[]) => v.map((s, i) => s.padEnd(widths[i]!)).join("  ");
  console.log("   " + fmt(cols));
  rs.forEach((r, i) => {
    console.log((ok(r) ? "ok " : "!! ") + fmt(vals[i]!));
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
  for (const d of opts.distro ?? []) {
    if (!(d in DISTROS)) {
      console.error(`Error: unknown distro '${d}' (one of: ${Object.keys(DISTROS).join(", ")})`);
      process.exit(2);
    }
  }
  const rs = await rows(opts.distro ?? Object.keys(DISTROS), opts.package ?? PACKAGES);
  if (opts.json === true) {
    console.log(JSON.stringify(rs, undefined, 2));
  } else {
    printTable(rs);
  }
  process.exitCode = rs.every(ok) ? 0 : 1;
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
