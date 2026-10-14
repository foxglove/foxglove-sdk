import {
  DISTROS,
  aptMatchesRelease,
  binaryJobName,
  debName,
  errorText,
  fetchFailure,
  jenkinsStatusFromApi,
  parseDistribution,
  parsePackagesIndex,
  resolveSelection,
  rowOk,
  rows,
  type Row,
  type Target,
} from "./rosPackageStatusReport";

function target(distro: string, arch: string): Target {
  const found = DISTROS[distro]?.targets.find((item) => item.arch === arch);
  if (found == undefined) {
    throw new Error(`missing ${distro} ${arch} target`);
  }
  return found;
}

describe("debName", () => {
  it("turns rosdistro package names into apt names", () => {
    expect(debName("humble", "foxglove_bridge")).toBe("ros-humble-foxglove-bridge");
    expect(debName("rolling", "foxglove_msgs")).toBe("ros-rolling-foxglove-msgs");
  });
});

describe("binaryJobName", () => {
  it("matches build.ros2.org binary job names, including irregular arm64 tokens", () => {
    expect(binaryJobName("humble", "foxglove_bridge", target("humble", "amd64"))).toBe(
      "Hbin_uJ64__foxglove_bridge__ubuntu_jammy_amd64__binary",
    );
    expect(binaryJobName("humble", "foxglove_msgs", target("humble", "arm64"))).toBe(
      "Hbin_ujv8_uJv8__foxglove_msgs__ubuntu_jammy_arm64__binary",
    );
    expect(binaryJobName("jazzy", "foxglove_bridge", target("jazzy", "arm64"))).toBe(
      "Jbin_unv8_uNv8__foxglove_bridge__ubuntu_noble_arm64__binary",
    );
    expect(binaryJobName("kilted", "foxglove_bridge", target("kilted", "amd64"))).toBe(
      "Kbin_uN64__foxglove_bridge__ubuntu_noble_amd64__binary",
    );
    // lyrical arm64 is `armv8_uRv8`; rolling arm64 is `unv8_uRv8`.
    expect(binaryJobName("lyrical", "foxglove_bridge", target("lyrical", "arm64"))).toBe(
      "Lbin_armv8_uRv8__foxglove_bridge__ubuntu_resolute_arm64__binary",
    );
    expect(binaryJobName("rolling", "foxglove_msgs", target("rolling", "arm64"))).toBe(
      "Rbin_unv8_uRv8__foxglove_msgs__ubuntu_resolute_arm64__binary",
    );
  });

  it("rejects an unknown distro", () => {
    expect(() => binaryJobName("iron", "foxglove_bridge", target("humble", "amd64"))).toThrow(
      /unknown distro 'iron'/,
    );
  });
});

describe("parsePackagesIndex", () => {
  it("reads Package/Version stanzas and does not let dbgsym clobber the binary", () => {
    const index = [
      "Package: ros-humble-foxglove-bridge",
      "Version: 3.6.0-1jammy.20261013.222750",
      "Description: bridge",
      "",
      "Package: ros-humble-foxglove-bridge-dbgsym",
      "Version: 3.6.0-1jammy.20261013.222750",
      "",
      "Package: ros-humble-no-version",
      "Architecture: amd64",
      "",
    ].join("\n");
    expect(parsePackagesIndex(index)).toEqual({
      "ros-humble-foxglove-bridge": "3.6.0-1jammy.20261013.222750",
      "ros-humble-foxglove-bridge-dbgsym": "3.6.0-1jammy.20261013.222750",
    });
  });
});

describe("parseDistribution", () => {
  it("keys a multi-package release by package name, not the repository name", () => {
    expect(
      parseDistribution({
        repositories: {
          "foxglove-sdk": {
            release: {
              version: "3.6.0-1",
              packages: ["foxglove_bridge", "foxglove_msgs"],
            },
          },
          docs_only: { doc: { type: "git" } },
          single: { release: { version: "1.2.3-1" } },
          empty: { release: { version: "9.9.9-1", packages: [] } },
        },
      }),
    ).toEqual({
      foxglove_bridge: "3.6.0-1",
      foxglove_msgs: "3.6.0-1",
      single: "1.2.3-1",
    });
  });

  it("returns an empty map when the distro document has no repositories", () => {
    expect(parseDistribution({})).toEqual({});
  });

  it("rejects a document that is not a mapping", () => {
    expect(() => parseDistribution("nope")).toThrow(/not a mapping/);
    expect(() => parseDistribution(null)).toThrow(/not a mapping/);
  });
});

describe("aptMatchesRelease", () => {
  it("accepts the rosdistro version with a codename and timestamp suffix", () => {
    expect(aptMatchesRelease("3.6.0-1jammy.20261013.222750", "3.6.0-1")).toBe(true);
    expect(aptMatchesRelease("3.6.0-1", "3.6.0-1")).toBe(true);
    expect(aptMatchesRelease("1:3.6.0-1resolute.20261010.151810", "3.6.0-1")).toBe(true);
  });

  it("rejects a longer debian revision and other mismatches", () => {
    expect(aptMatchesRelease("3.6.0-10jammy.20261013.222750", "3.6.0-1")).toBe(false);
    expect(aptMatchesRelease("3.5.0-1noble.20260915.110632", "3.6.0-1")).toBe(false);
    expect(aptMatchesRelease(undefined, "3.6.0-1")).toBe(false);
    expect(aptMatchesRelease("3.6.0-1jammy.1", undefined)).toBe(false);
  });
});

describe("jenkinsStatusFromApi", () => {
  it("summarizes job state, preferring disabled over an in-progress build", () => {
    expect(
      jenkinsStatusFromApi(
        JSON.stringify({ disabled: true, lastBuild: { building: true, result: null } }),
      ),
    ).toBe("disabled");
    expect(
      jenkinsStatusFromApi(JSON.stringify({ lastBuild: { building: true, result: null } })),
    ).toBe("building");
    expect(
      jenkinsStatusFromApi(JSON.stringify({ inQueue: true, lastBuild: { result: "SUCCESS" } })),
    ).toBe("queued");
    expect(jenkinsStatusFromApi(JSON.stringify({ lastBuild: { result: "SUCCESS" } }))).toBe(
      "success",
    );
    expect(jenkinsStatusFromApi(JSON.stringify({ lastBuild: { result: "ABORTED" } }))).toBe(
      "aborted",
    );
    expect(jenkinsStatusFromApi(JSON.stringify({ lastBuild: null }))).toBe("never-built");
  });

  it("rejects a non-object or unparseable job document", () => {
    expect(() => jenkinsStatusFromApi("[]")).toThrow(/not a JSON object/);
    expect(() => jenkinsStatusFromApi("not-json")).toThrow(SyntaxError);
  });
});

describe("resolveSelection", () => {
  it("accepts known distros and packages", () => {
    const selection = resolveSelection(["humble"], ["foxglove_msgs"]);
    expect(selection.packages).toEqual(["foxglove_msgs"]);
    expect(selection.distros.map((distro) => distro.name)).toEqual(["humble"]);
  });

  it("rejects an unknown distro or package with one message", () => {
    expect(() => resolveSelection(["nope"], ["foxglove_bridge"])).toThrow(
      /unknown distro 'nope' \(one of: humble, jazzy, kilted, lyrical, rolling\)/,
    );
    expect(() => resolveSelection(["humble"], ["foxglove-bridge"])).toThrow(
      /unknown package 'foxglove-bridge' \(one of: foxglove_bridge, foxglove_msgs\)/,
    );
  });
});

describe("fetchFailure", () => {
  const url = "https://build.ros2.org/job/example/api/json";

  it("includes the URL for Error and non-Error causes", () => {
    expect(fetchFailure(url, new Error("The operation was aborted due to timeout"))).toBe(
      `GET ${url}: The operation was aborted due to timeout`,
    );
    expect(fetchFailure(url, "fetch failed")).toBe(`GET ${url}: fetch failed`);
    expect(errorText("fetch failed")).toBe("fetch failed");
    expect(errorText(new Error("HTTP 500"))).toBe("HTTP 500");
  });

  it("does not prefix a message that already names the URL", () => {
    expect(fetchFailure(url, new Error(`GET ${url}: HTTP 500`))).toBe(`GET ${url}: HTTP 500`);
  });
});

describe("rowOk", () => {
  const base: Row = {
    distro: "rolling",
    package: "foxglove_bridge",
    target: "resolute/amd64",
    rosdistro: "3.6.0-1",
    testing: "3.6.0-1resolute.20261010.151810",
    main: undefined,
    jenkins: "success",
    job_url: "https://build.ros2.org/job/example",
  };

  it("is ok when testing matches the release and Jenkins succeeded, even if main has not synced", () => {
    expect(rowOk(base)).toBe(true);
  });

  it("is not ok when the testing version, the Jenkins result, or a fetch fails", () => {
    expect(rowOk({ ...base, testing: "3.5.0-1resolute.20260101.000000" })).toBe(false);
    expect(rowOk({ ...base, jenkins: "failure" })).toBe(false);
    expect(rowOk({ ...base, rosdistro: undefined })).toBe(false);
    expect(
      rowOk({
        ...base,
        jenkins: "error",
        error: "GET https://build.ros2.org/job/example: The operation was aborted due to timeout",
      }),
    ).toBe(false);
  });
});

describe("rows", () => {
  it("does not gate the row when the main repo fetch fails", async () => {
    const mainError =
      "GET https://repo.ros2.org/ubuntu/main/dists/noble/main/binary-amd64/Packages.gz: The operation was aborted due to timeout";
    const selection = resolveSelection(["jazzy"], ["foxglove_bridge"]);
    const result = await rows(
      selection.distros,
      selection.packages,
      {
        rosdistroVersions: async () => ({ value: { foxglove_bridge: "3.6.0-1" } }),
        packagesIndex: async (_os, component) =>
          component === "main"
            ? { error: mainError }
            : { value: { "ros-jazzy-foxglove-bridge": "3.6.0-1noble.20260930.003057" } },
        jenkinsStatus: async () => ({ value: "success" }),
      },
      "https://build.ros2.org",
    );

    expect(result).toHaveLength(2);
    for (const row of result) {
      expect(row.main).toBeUndefined();
      expect(row.main_error).toBe(mainError);
      expect(row.error).toBeUndefined();
      expect(row.jenkins).toBe("success");
      expect(rowOk(row)).toBe(true);
    }
  });
});
