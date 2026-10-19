import { glob } from "glob";
import { spawnSync } from "node:child_process";
import { appendFile, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import semver from "semver";

// Captures the quoted value of the first version assignment at the start of a line.
const versionRegex = /^version\s*=\s*"([^"]*)"/m;
// Matches each inline foxglove or foxglove_derive version and captures the prefix to preserve it during replacement.
const dependencyVersionRegex = /^(foxglove(?:_derive)?\s*=\s*\{[^\n]*?version\s*=\s*)"[^"]*"/gm;

async function main(
  newVersionV = process.argv[2],
  workspaceRoot = path.resolve(__dirname, ".."),
): Promise<void> {
  if (newVersionV?.startsWith("v") !== true) {
    console.log("Usage: bumpSdkVersion.ts <version>");
    console.log("<version> must start with 'v'");
    process.exit(1);
  }

  // Remove the 'v' prefix
  const newVersion = newVersionV.slice(1);
  if (!semver.valid(newVersion)) {
    console.log("Usage: bumpSdkVersion.ts <version>");
    console.log(`"${newVersionV}" is not a valid semver version`);
    process.exit(1);
  }

  // Find all Cargo.toml files in the workspace
  const cargoFiles = await glob("**/Cargo.toml", {
    // foxglove_data_loader is versioned separately
    ignore: ["**/target/**", "**/node_modules/**", "cpp/build/**", "rust/foxglove_data_loader/**"],
    cwd: workspaceRoot,
    absolute: true,
  });

  let success = true;
  let prevVersion: string | undefined;

  for (const cargoFile of cargoFiles) {
    console.log(`Checking ${cargoFile}...`);
    const content = await readFile(cargoFile, "utf8");

    if (!versionRegex.test(content)) {
      console.log(`  ℹ️ Skipped Cargo version update; does not contain version field`);
      continue;
    }

    prevVersion = versionRegex.exec(content)?.[1] ?? "";

    // check that newVersion is greater than prevVersion
    if (semver.compare(newVersion, prevVersion) <= 0) {
      console.error(
        `  ❌ New version ${newVersion} must be greater than previous version ${prevVersion}`,
      );
      success = false;
      continue;
    }

    const updatedContent = content
      .replace(versionRegex, `version = "${newVersion}"`)
      .replace(dependencyVersionRegex, `$1"${newVersion}"`);
    if (content === updatedContent) {
      console.error(`  ❌ Version could not be updated from "${prevVersion}" to "${newVersion}"`);
      success = false;
    } else {
      await writeFile(cargoFile, updatedContent);
      console.log(`  ✅ Updated version in ${cargoFile} to ${newVersion}`);
    }
  }

  if (!success || !prevVersion) {
    console.error("\n❌ Some versions could not be updated");
    process.exit(1);
  }

  // run cargo tree --workspace
  // we don't need the output, only check the exit code
  console.log("\nValidating Cargo.toml...");
  const { status, stderr } = spawnSync("cargo", ["tree", "--workspace"], {
    cwd: workspaceRoot,
  });

  if (status !== 0) {
    console.error(stderr.toString());
    console.error("\n❌ Failed to validate Cargo.toml");
    process.exit(status);
  }

  // update version in Python & C++ SDK docs
  console.log("\nUpdating version string in SDK docs...");
  const pythonVersionModule = path.join(
    workspaceRoot,
    "python/foxglove-sdk/python/docs/version.py",
  );
  const cppVersionModule = path.join(workspaceRoot, "cpp/foxglove/docs/version.py");

  for (const module of [pythonVersionModule, cppVersionModule]) {
    const content = await readFile(module, "utf8");
    const updatedContent = content.replace(
      /SDK_VERSION\s*=\s*"([^"]*)"/m,
      `SDK_VERSION = "${newVersion}"`,
    );
    if (!updatedContent.includes(newVersion)) {
      console.error(`❌ Failed to update docs version in ${module}`);
      process.exit(1);
    }
    await writeFile(module, updatedContent);
    console.log(`  ✅ Updated version in ${module} to ${newVersion}`);
  }

  // update version in C++ CMakeLists.txt
  console.log("\nUpdating version in C++ CMakeLists.txt...");
  const cmakeFile = path.join(workspaceRoot, "cpp/CMakeLists.txt");
  {
    const content = await readFile(cmakeFile, "utf8");
    const updatedContent = content.replace(
      /project\(foxglove-sdk VERSION [^)]+\)/m,
      `project(foxglove-sdk VERSION ${newVersion})`,
    );
    if (!updatedContent.includes(newVersion)) {
      console.error(`❌ Failed to update version in ${cmakeFile}`);
      process.exit(1);
    }
    await writeFile(cmakeFile, updatedContent);
    console.log(`  ✅ Updated version in ${cmakeFile} to ${newVersion}`);
  }

  console.log("\n✅ Success!");

  // github action outputs
  const githubOutput = process.env.GITHUB_OUTPUT;
  if (githubOutput) {
    await appendFile(
      path.resolve(githubOutput),
      `prev-version=${prevVersion}\nnew-version=${newVersion}\n`,
    );
  }
}

if (require.main === module) {
  main().catch((err: unknown) => {
    console.error(err);
    process.exit(1);
  });
}

export const testOnlyExports = { main };
