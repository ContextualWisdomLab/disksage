#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const MAX_INPUT_BYTES = 128 * 1024 * 1024;
const SPDX_VERSION = "SPDX-2.3";
const TOOL_NAME = "disksage-release-sbom";

function usage() {
  return [
    "usage: generate-release-sbom.mjs --cargo-metadata FILE --npm-lock FILE --source-revision HEX --created ISO --output FILE",
    "       generate-release-sbom.mjs --validate FILE",
  ].join("\n");
}

function parseArgs(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--help" || flag === "-h") throw new Error("help");
    if (!flag.startsWith("--")) throw new Error("unknown argument");
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`${flag}-value-missing`);
    if (values.has(flag)) throw new Error(`${flag}-duplicate`);
    values.set(flag, value);
    index += 1;
  }
  if (values.has("--validate")) {
    if (values.size !== 1) throw new Error("validate-cannot-be-combined");
    return { validate: values.get("--validate") };
  }
  const required = ["--cargo-metadata", "--npm-lock", "--source-revision", "--created", "--output"];
  if (required.some((flag) => !values.has(flag))) throw new Error("required-argument-missing");
  return {
    cargoMetadata: values.get("--cargo-metadata"),
    npmLock: values.get("--npm-lock"),
    sourceRevision: values.get("--source-revision"),
    created: values.get("--created"),
    output: values.get("--output"),
  };
}

function readJson(path) {
  const absolutePath = resolve(path);
  const size = statSync(absolutePath).size;
  if (size > MAX_INPUT_BYTES) throw new Error("input-too-large");
  return JSON.parse(readFileSync(absolutePath, "utf8"));
}

function id(prefix, value) {
  return `SPDXRef-${prefix}-${createHash("sha256").update(value).digest("hex").slice(0, 24)}`;
}

function packageRecord(spdxId, name, version, downloadLocation, licenseDeclared) {
  return {
    SPDXID: spdxId,
    name,
    versionInfo: version,
    downloadLocation: downloadLocation || "NOASSERTION",
    filesAnalyzed: false,
    licenseConcluded: "NOASSERTION",
    licenseDeclared: licenseDeclared || "NOASSERTION",
    supplier: "NOASSERTION",
  };
}

function cargoPackages(metadata) {
  if (!Array.isArray(metadata.packages) || metadata.packages.length === 0) {
    throw new Error("cargo-metadata-packages-missing");
  }
  const records = new Map();
  const ids = new Map();
  for (const pkg of metadata.packages) {
    if (!pkg?.id || !pkg.name || !pkg.version) throw new Error("cargo-package-shape-invalid");
    const spdxId = id("Cargo", String(pkg.id));
    ids.set(String(pkg.id), spdxId);
    records.set(spdxId, packageRecord(
      spdxId,
      `cargo:${pkg.name}`,
      String(pkg.version),
      pkg.source ? String(pkg.source) : "NOASSERTION",
      pkg.license ? String(pkg.license) : "NOASSERTION",
    ));
  }
  return { records, ids };
}

function npmPackages(lockfile) {
  if (!lockfile || ![2, 3].includes(lockfile.lockfileVersion) || !lockfile.packages) {
    throw new Error("npm-lock-shape-invalid");
  }
  const records = new Map();
  const pathIds = new Map();
  for (const [packagePath, pkg] of Object.entries(lockfile.packages)) {
    if (!packagePath || !pkg?.version) continue;
    const name = packagePath.split("node_modules/").at(-1);
    if (!name || name.startsWith(".")) throw new Error("npm-package-name-invalid");
    const spdxId = id("Npm", `${packagePath}\0${pkg.version}`);
    pathIds.set(packagePath, spdxId);
    records.set(spdxId, packageRecord(
      spdxId,
      `npm:${name}`,
      String(pkg.version),
      pkg.resolved ? String(pkg.resolved) : "NOASSERTION",
      pkg.license ? String(pkg.license) : "NOASSERTION",
    ));
  }
  return { records, pathIds };
}

function resolveNpmDependencyPath(packages, packagePath, dependencyName) {
  let scope = packagePath;
  while (true) {
    const candidate = scope
      ? `${scope}/node_modules/${dependencyName}`
      : `node_modules/${dependencyName}`;
    if (packages[candidate]?.version) return candidate;

    const parentBoundary = scope.lastIndexOf("/node_modules/");
    if (parentBoundary >= 0) {
      scope = scope.slice(0, parentBoundary);
      continue;
    }
    if (scope) {
      scope = "";
      continue;
    }
    return null;
  }
}

function addRelationship(relationships, left, relationship, right) {
  if (!left || !right || left === right) return;
  relationships.set(`${left}\0${relationship}\0${right}`, {
    spdxElementId: left,
    relationshipType: relationship,
    relatedSpdxElement: right,
  });
}

function buildDocument(args, metadata, lockfile) {
  if (!/^[0-9a-f]{7,64}$/.test(args.sourceRevision)) throw new Error("source-revision-invalid");
  const createdDate = new Date(args.created);
  if (Number.isNaN(createdDate.valueOf())) {
    throw new Error("created-timestamp-invalid");
  }
  const created = createdDate.toISOString();
  const cargo = cargoPackages(metadata);
  const npm = npmPackages(lockfile);
  const packages = new Map([...cargo.records, ...npm.records]);
  const rootCargoId = metadata.resolve?.root;
  const rootSpdxId = cargo.ids.get(String(rootCargoId))
    ?? [...cargo.records.values()].find((pkg) => pkg.name === "cargo:disksage")?.SPDXID;
  if (!rootSpdxId) throw new Error("cargo-root-package-missing");

  const relationships = new Map();
  const nodes = metadata.resolve?.nodes;
  if (Array.isArray(nodes)) {
    for (const node of nodes) {
      const left = cargo.ids.get(String(node.id));
      for (const dependency of node.dependencies ?? []) {
        const right = cargo.ids.get(String(dependency.pkg ?? dependency));
        addRelationship(relationships, left, "DEPENDS_ON", right);
      }
    }
  }
  for (const [packagePath, pkg] of Object.entries(lockfile.packages)) {
    if (!packagePath || !pkg?.version) continue;
    const left = npm.pathIds.get(packagePath);
    for (const dependencyName of Object.keys(pkg.dependencies ?? {})) {
      const dependencyPath = resolveNpmDependencyPath(lockfile.packages, packagePath, dependencyName);
      const right = dependencyPath ? npm.pathIds.get(dependencyPath) : undefined;
      addRelationship(relationships, left, "DEPENDS_ON", right);
    }
  }
  const namespace = `https://github.com/ContextualWisdomLab/disksage/sbom/${args.sourceRevision}`;
  return {
    spdxVersion: SPDX_VERSION,
    dataLicense: "CC0-1.0",
    SPDXID: "SPDXRef-DOCUMENT",
    name: `disksage-${args.sourceRevision}`,
    documentNamespace: namespace,
    creationInfo: {
      created,
      creators: [`Tool: ${TOOL_NAME}`],
    },
    documentDescribes: [rootSpdxId],
    packages: [...packages.values()].sort((left, right) => left.SPDXID.localeCompare(right.SPDXID)),
    relationships: [...relationships.values()].sort((left, right) => {
      const leftKey = `${left.spdxElementId}\0${left.relationshipType}\0${left.relatedSpdxElement}`;
      const rightKey = `${right.spdxElementId}\0${right.relationshipType}\0${right.relatedSpdxElement}`;
      return leftKey.localeCompare(rightKey);
    }),
    documentComment: `Dependency inventory bound to source revision ${args.sourceRevision}.`,
  };
}

function validateDocument(document, expectedRevision = null) {
  if (document?.spdxVersion !== SPDX_VERSION || document?.SPDXID !== "SPDXRef-DOCUMENT") {
    throw new Error("spdx-document-header-invalid");
  }
  if (!/^https:\/\/github\.com\/ContextualWisdomLab\/disksage\/sbom\/[0-9a-f]{7,64}$/.test(document.documentNamespace)) {
    throw new Error("spdx-namespace-invalid");
  }
  const revision = document.documentNamespace.split("/").at(-1);
  if (expectedRevision && revision !== expectedRevision) throw new Error("spdx-source-revision-mismatch");
  if (!Array.isArray(document.packages) || document.packages.length === 0) throw new Error("spdx-packages-missing");
  const packageIds = new Set();
  for (const pkg of document.packages) {
    if (!pkg?.SPDXID || packageIds.has(pkg.SPDXID) || !pkg.name || !pkg.versionInfo || !pkg.downloadLocation) {
      throw new Error("spdx-package-invalid");
    }
    packageIds.add(pkg.SPDXID);
  }
  if (!Array.isArray(document.documentDescribes) || document.documentDescribes.length !== 1
      || !packageIds.has(document.documentDescribes[0])) {
    throw new Error("spdx-document-describes-invalid");
  }
  const serialized = JSON.stringify(document);
  if (/\/Users\/|\/home\/runner\/|\/private\/tmp\/|[A-Za-z]:\\/.test(serialized)) {
    throw new Error("spdx-private-path-present");
  }
  for (const relationship of document.relationships ?? []) {
    if (!packageIds.has(relationship.spdxElementId) || !packageIds.has(relationship.relatedSpdxElement)) {
      throw new Error("spdx-relationship-invalid");
    }
  }
  return document;
}

function main(argv) {
  const args = parseArgs(argv);
  if (args.validate) {
    validateDocument(readJson(args.validate));
    return;
  }
  const document = buildDocument(args, readJson(args.cargoMetadata), readJson(args.npmLock));
  validateDocument(document, args.sourceRevision);
  const output = resolve(args.output);
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, `${JSON.stringify(document, null, 2)}\n`, { mode: 0o644 });
}

try {
  main(process.argv.slice(2));
} catch (error) {
  if (error instanceof Error && error.message === "help") {
    console.log(usage());
    process.exit(0);
  }
  console.error(`error:${error instanceof Error ? error.message : "unknown"}`);
  process.exit(2);
}
