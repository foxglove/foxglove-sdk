// Pure parsing and classification for `yarn ros-package-status`.
// Kept free of network and CLI side effects so it can be unit tested.
// The command itself is scripts/rosPackageStatus.ts.

// Packages we maintain. Must match rosdistro package names.
export const PACKAGES = ["foxglove_bridge", "foxglove_msgs"];

export interface Target {
  os: string;
  codename: string;
  arch: string;
  // The token after "bin_" in the Jenkins job name, e.g.
  //   Kbin_uN64__foxglove_bridge__ubuntu_noble_amd64__binary
  // It is irregular across targets, so list it explicitly. To see all jobs:
  //   curl -sg 'https://build.ros2.org/api/json?tree=jobs[name]'
  jenkins: string;
}

export interface Distro {
  // First letter of buildfarm job names.
  prefix: string;
  targets: Target[];
}

export const DISTROS: Record<string, Distro> = {
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

export interface Loaded<T> {
  value?: T;
  error?: string;
}

export interface Row {
  distro: string;
  package: string;
  target: string;
  rosdistro: string | undefined;
  testing: string | undefined;
  /** Apt version in the main repo. A fetch failure is `main_error`, not this field. */
  main: string | undefined;
  jenkins: string;
  job_url: string;
  /** Main apt index fetch failure, including the URL. Does not gate the row. */
  main_error?: string;
  /** rosdistro, testing, or Jenkins fetch failure, including the URL. */
  error?: string;
}

export interface ResolvedDistro {
  name: string;
  info: Distro;
}

/** Error for a distro name that is not in DISTROS. Lists the known names. */
function unknownDistro(name: string): Error {
  return new Error(`unknown distro '${name}' (one of: ${Object.keys(DISTROS).join(", ")})`);
}

/**
 * The CLI's distro and package check. Unknown names throw; the CLI prints
 * that message and exits 1.
 */
export function resolveSelection(
  distros: readonly string[],
  packages: readonly string[],
): { distros: ResolvedDistro[]; packages: string[] } {
  const resolved: ResolvedDistro[] = [];
  for (const name of distros) {
    const info = DISTROS[name];
    if (info == undefined) {
      throw unknownDistro(name);
    }
    resolved.push({ name, info });
  }
  for (const pkg of packages) {
    if (!PACKAGES.includes(pkg)) {
      throw new Error(`unknown package '${pkg}' (one of: ${PACKAGES.join(", ")})`);
    }
  }
  return { distros: resolved, packages: [...packages] };
}

/** `err.message` for Error values, otherwise String(err). */
export function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Failure text that always names the request URL. */
export function fetchFailure(url: string, err: unknown): string {
  const message = errorText(err);
  return message.startsWith(`GET ${url}:`) ? message : `GET ${url}: ${message}`;
}

export function debName(distro: string, pkg: string): string {
  return `ros-${distro}-${pkg.replace(/_/g, "-")}`;
}

export function binaryJobName(distro: string, pkg: string, target: Target): string {
  const info = DISTROS[distro];
  if (info == undefined) {
    throw unknownDistro(distro);
  }
  return `${info.prefix}bin_${target.jenkins}__${pkg}__${target.os}_${target.codename}_${target.arch}__binary`;
}

/** debian package name -> Version, from a binary Packages index (already decompressed). */
export function parsePackagesIndex(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  let name: string | undefined;
  for (const line of text.split("\n")) {
    if (line.startsWith("Package: ")) {
      name = line.slice("Package: ".length).trim();
    } else if (line.startsWith("Version: ") && name != undefined) {
      out[name] = line.slice("Version: ".length).trim();
    } else if (line.trim() === "") {
      name = undefined;
    }
  }
  return out;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value != null && !Array.isArray(value);
}

/**
 * package -> release version ('3.6.0-1') from a parsed distribution.yaml.
 * Repositories with an explicit `release.packages` list (foxglove-sdk publishes
 * both foxglove_bridge and foxglove_msgs) are keyed by those names, not the
 * repository name.
 */
export function parseDistribution(doc: unknown): Record<string, string> {
  if (!isRecord(doc)) {
    throw new Error("distribution.yaml is not a mapping");
  }
  const repositories = doc.repositories;
  if (repositories == null) {
    return {};
  }
  if (!isRecord(repositories)) {
    throw new Error("distribution.yaml repositories is not a mapping");
  }
  const out: Record<string, string> = {};
  for (const [repoName, repo] of Object.entries(repositories)) {
    if (!isRecord(repo) || !isRecord(repo.release)) {
      continue;
    }
    const version = repo.release.version;
    if (typeof version !== "string" || version === "") {
      continue;
    }
    const listed = repo.release.packages;
    const names = Array.isArray(listed) ? listed : [repoName];
    for (const pkg of names) {
      if (typeof pkg === "string" && pkg !== "") {
        out[pkg] = version;
      }
    }
  }
  return out;
}

/**
 * True when `aptVersion` is the rosdistro release plus an optional non-numeric
 * build suffix. rosdistro stores `3.6.0-1`; apt appends the codename and a
 * timestamp (`3.6.0-1jammy.20261013.222750`). A longer Debian revision
 * (`3.6.0-10jammy...`) must not match `3.6.0-1`.
 */
export function aptMatchesRelease(
  aptVersion: string | undefined,
  releaseVersion: string | undefined,
): boolean {
  if (aptVersion == undefined || releaseVersion == undefined || releaseVersion === "") {
    return false;
  }
  const version = /^\d+:/.test(aptVersion)
    ? aptVersion.slice(aptVersion.indexOf(":") + 1)
    : aptVersion;
  if (!version.startsWith(releaseVersion)) {
    return false;
  }
  const next = version.charAt(releaseVersion.length);
  return next === "" || next < "0" || next > "9";
}

/**
 * One-word summary of a Jenkins job API body: disabled / building / queued / never-built / <result>.
 * `load` supplies `missing` when the job URL is a 404.
 */
export function jenkinsStatusFromApi(body: string): string {
  const parsed: unknown = JSON.parse(body);
  if (!isRecord(parsed)) {
    throw new Error("Jenkins job API response is not a JSON object");
  }
  const lastBuild = isRecord(parsed.lastBuild) ? parsed.lastBuild : {};
  if (parsed.disabled === true) {
    return "disabled";
  }
  if (lastBuild.building === true) {
    return "building";
  }
  if (parsed.inQueue === true) {
    return "queued";
  }
  const result = lastBuild.result;
  if (typeof result !== "string" || result === "") {
    return "never-built";
  }
  return result.toLowerCase();
}

/** Testing has the released version and the last build succeeded. Main may lag or fail to fetch. */
export function rowOk(row: Row): boolean {
  return (
    row.error == undefined &&
    aptMatchesRelease(row.testing, row.rosdistro) &&
    row.jenkins === "success"
  );
}

export interface StatusSources {
  rosdistroVersions: (distro: string) => Promise<Loaded<Record<string, string>>>;
  packagesIndex: (
    os: string,
    component: string,
    codename: string,
    arch: string,
  ) => Promise<Loaded<Record<string, string>>>;
  jenkinsStatus: (job: string) => Promise<Loaded<string>>;
}

function rowErrors(parts: Loaded<unknown>[]): string | undefined {
  const messages = parts.flatMap((part) => (part.error == undefined ? [] : [part.error]));
  return messages.length > 0 ? messages.join("; ") : undefined;
}

/** One row per distro, package, and target. A main-repo fetch failure does not gate the row. */
export async function rows(
  distros: readonly ResolvedDistro[],
  packages: readonly string[],
  sources: StatusSources,
  jenkinsBase: string,
): Promise<Row[]> {
  const pending: Promise<Row>[] = [];
  for (const { name: distro, info } of distros) {
    for (const pkg of packages) {
      for (const target of info.targets) {
        const deb = debName(distro, pkg);
        const job = binaryJobName(distro, pkg, target);
        pending.push(
          (async (): Promise<Row> => {
            const [expected, testing, mainRepo, status] = await Promise.all([
              sources.rosdistroVersions(distro),
              sources.packagesIndex(target.os, "testing", target.codename, target.arch),
              sources.packagesIndex(target.os, "main", target.codename, target.arch),
              sources.jenkinsStatus(job),
            ]);
            const error = rowErrors([expected, testing, status]);
            return {
              distro,
              package: pkg,
              target: `${target.codename}/${target.arch}`,
              rosdistro: expected.value?.[pkg],
              testing: testing.value?.[deb],
              main: mainRepo.value?.[deb],
              jenkins: status.value ?? "error",
              job_url: `${jenkinsBase}/job/${job}`,
              ...(mainRepo.error == undefined ? {} : { main_error: mainRepo.error }),
              ...(error == undefined ? {} : { error }),
            };
          })(),
        );
      }
    }
  }
  return await Promise.all(pending);
}
