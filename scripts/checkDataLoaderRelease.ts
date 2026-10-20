import { spawnSync } from "node:child_process";
import path from "node:path";
import semver from "semver";

type Release = {
  version: string;
  sdkRequirement: string;
};

const indexUrl = "https://index.crates.io/fo/xg/foxglove_data_loader";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value != undefined && !Array.isArray(value);
}

function parseVersion(value: unknown): semver.SemVer {
  if (typeof value !== "string") {
    throw new Error("Missing release version");
  }
  const version = semver.parse(value);
  if (version == undefined || version.version !== value) {
    throw new Error(`Invalid release version ${value}`);
  }
  return version;
}

function compatibilityLine(version: semver.SemVer): string {
  if (version.major > 0) {
    return `${version.major}`;
  }
  if (version.minor > 0) {
    return `0.${version.minor}`;
  }
  return `0.0.${version.patch}`;
}

function sdkCompatibilityLine(requirement: string): string {
  const value = requirement.startsWith("^") ? requirement.slice(1) : requirement;
  const version = semver.parse(value);
  if (version == undefined || version.version !== value || version.prerelease.length > 0) {
    throw new Error(
      `Unsupported Foxglove requirement ${requirement}; expected a stable caret version`,
    );
  }
  return compatibilityLine(version);
}

function readRelease(version: unknown, dependencies: unknown): Release {
  const parsedVersion = parseVersion(version);
  if (!Array.isArray(dependencies)) {
    throw new Error(`Missing dependencies for loader ${parsedVersion.version}`);
  }
  const sdkDependencies = dependencies.filter(
    (dependency: unknown): dependency is Record<string, unknown> =>
      isRecord(dependency) &&
      (dependency.package ?? dependency.name) === "foxglove" &&
      (dependency.kind == undefined || dependency.kind === "normal"),
  );
  if (sdkDependencies.length !== 1 || typeof sdkDependencies[0]?.req !== "string") {
    throw new Error(`Expected one Foxglove requirement for loader ${parsedVersion.version}`);
  }
  return { version: parsedVersion.version, sdkRequirement: sdkDependencies[0].req };
}

function parseMetadata(text: string): Release {
  const metadata: unknown = JSON.parse(text);
  if (!isRecord(metadata) || !Array.isArray(metadata.packages)) {
    throw new Error("Cargo metadata is missing packages");
  }
  const loader: unknown = metadata.packages.find(
    (entry: unknown) => isRecord(entry) && entry.name === "foxglove_data_loader",
  );
  if (!isRecord(loader)) {
    throw new Error("Cargo metadata is missing foxglove_data_loader");
  }
  return readRelease(loader.version, loader.dependencies);
}

function parseIndex(text: string): Release[] {
  if (text.trim() === "") {
    throw new Error("The loader registry index is empty");
  }
  return text
    .trim()
    .split("\n")
    .map((line) => {
      const entry: unknown = JSON.parse(line);
      if (!isRecord(entry) || entry.name !== "foxglove_data_loader") {
        throw new Error("Invalid loader registry entry");
      }
      return readRelease(entry.vers, entry.deps);
    });
}

function checkCompatibility(candidate: Release, published: readonly Release[]): void {
  const version = parseVersion(candidate.version);
  const line = compatibilityLine(version);
  const sdkLine = sdkCompatibilityLine(candidate.sdkRequirement);
  for (const release of published) {
    const previous = parseVersion(release.version);
    if (previous.prerelease.length > 0 || compatibilityLine(previous) !== line) {
      continue;
    }
    if (sdkCompatibilityLine(release.sdkRequirement) !== sdkLine) {
      let nextVersion: string;
      if (version.major > 0) {
        nextVersion = `${version.major + 1}.0.0`;
      } else if (version.minor > 0) {
        nextVersion = `0.${version.minor + 1}.0`;
      } else {
        nextVersion = `0.0.${version.patch + 1}`;
      }
      throw new Error(
        `Cannot publish foxglove_data_loader ${candidate.version} with foxglove ${candidate.sdkRequirement}.\n` +
          `Published loader ${release.version} requires foxglove ${release.sdkRequirement}.\n` +
          `Increase the loader version to ${nextVersion}.`,
      );
    }
  }
}

async function main(): Promise<void> {
  const result = spawnSync(
    "cargo",
    ["metadata", "--locked", "--no-deps", "--format-version", "1"],
    {
      cwd: path.resolve(__dirname, ".."),
      encoding: "utf8",
      timeout: 30_000,
      maxBuffer: 10 * 1024 * 1024,
    },
  );
  if (result.error != undefined) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`Unable to read Cargo metadata: ${result.stderr}`);
  }
  const candidate = parseMetadata(result.stdout);
  const response = await fetch(indexUrl, {
    headers: { "User-Agent": "foxglove-sdk-release-check" },
    signal: AbortSignal.timeout(15_000),
  });
  if (!response.ok) {
    throw new Error(`Unable to read ${indexUrl}: HTTP ${response.status}`);
  }
  checkCompatibility(candidate, parseIndex(await response.text()));
  console.log(
    `Loader ${candidate.version} with Foxglove ${candidate.sdkRequirement} passes the release compatibility check`,
  );
}

export const testOnlyExports = { checkCompatibility, parseMetadata, parseIndex, main };

if (require.main === module) {
  main().catch((err: unknown) => {
    console.error(err);
    process.exitCode = 1;
  });
}
