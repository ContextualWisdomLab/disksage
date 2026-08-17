import path from 'node:path';

export const REPOSITORY_WORKFLOW_PREFIX = '.github/workflows/';

const NON_ACTIVE_WORKFLOW_STATES = new Set([
  'deleted',
  'disabled_fork',
  'disabled_inactivity',
  'disabled_manually',
]);

/** Normalize a GitHub Actions workflow path without changing its case authority. */
export function normalizeWorkflowPath(rawPath) {
  if (typeof rawPath !== 'string' || rawPath.length === 0) return null;
  const slashNormalized = rawPath.replaceAll('\\', '/');
  const withoutDotPrefix = slashNormalized.replace(/^(?:\.\/)+/, '');
  const normalized = path.posix.normalize(withoutDotPrefix);
  if (normalized === '.' || normalized.startsWith('../') || normalized.startsWith('/')) return null;
  return normalized;
}

function isRepositoryWorkflowPath(workflowPath) {
  return Boolean(
    workflowPath &&
    workflowPath.startsWith(REPOSITORY_WORKFLOW_PREFIX) &&
    /\.ya?ml$/.test(workflowPath),
  );
}

/** Classify one Actions registry record against exact protected-main and active-PR path authority. */
export function classifyWorkflowRecord(record, protectedWorkflowPaths, activePullRequestWorkflowPaths = new Set()) {
  if (
    !record ||
    typeof record !== 'object' ||
    !Number.isSafeInteger(record.id) ||
    record.id <= 0 ||
    typeof record.state !== 'string' ||
    typeof record.path !== 'string' ||
    record.path.length === 0
  ) {
    throw new Error('actions-workflow-record-invalid');
  }
  const normalizedPath = normalizeWorkflowPath(record.path);
  if (!normalizedPath) throw new Error('actions-workflow-record-invalid');
  if (record.state !== 'active') {
    if (!NON_ACTIVE_WORKFLOW_STATES.has(record.state)) {
      throw new Error('actions-workflow-record-invalid');
    }
    return { classification: 'disabled', path: normalizedPath, workflow_id: record.id };
  }
  if (!normalizedPath.startsWith(REPOSITORY_WORKFLOW_PREFIX)) {
    return { classification: 'github-dynamic', path: normalizedPath, workflow_id: record.id };
  }
  if (protectedWorkflowPaths.has(normalizedPath)) {
    return { classification: 'present', path: normalizedPath, workflow_id: record.id };
  }
  if (activePullRequestWorkflowPaths.has(normalizedPath)) {
    return {
      classification: 'unresolved',
      reason: 'active-pr-workflow',
      path: normalizedPath,
      workflow_id: record.id,
    };
  }
  return { classification: 'orphaned-deleted', path: normalizedPath, workflow_id: record.id };
}

/** Classify a complete collection of Actions registry records. */
export function classifyWorkflowRecords(records, protectedWorkflowPaths, activePullRequestWorkflowPaths = new Set()) {
  return records.map((record) =>
    classifyWorkflowRecord(record, protectedWorkflowPaths, activePullRequestWorkflowPaths),
  );
}

async function listAll(fetchJson, endpoint, invalidError) {
  const records = [];
  let page = 1;
  let pageSize;
  do {
    const payload = await fetchJson(`${endpoint}&per_page=100&page=${page}`);
    if (!Array.isArray(payload)) throw new Error(invalidError);
    records.push(...payload);
    pageSize = payload.length;
    page += 1;
  } while (pageSize === 100);
  return records;
}

/** Read every page of the repository Actions workflow registry without accepting partial snapshots. */
export async function listAllWorkflowRecords(fetchJson, repository) {
  const records = [];
  let page = 1;
  let expectedTotal = null;
  while (true) {
    const payload = await fetchJson(`/repos/${repository}/actions/workflows?per_page=100&page=${page}`);
    const totalCount = payload?.total_count;
    if (
      !payload ||
      !Array.isArray(payload.workflows) ||
      !Number.isSafeInteger(totalCount) ||
      totalCount < 0
    ) {
      throw new Error('actions-workflow-list-invalid');
    }
    if (expectedTotal === null) {
      expectedTotal = totalCount;
    } else if (totalCount !== expectedTotal) {
      throw new Error('actions-workflow-list-moved');
    }
    records.push(...payload.workflows);
    if (records.length === expectedTotal) return records;
    if (records.length > expectedTotal || payload.workflows.length < 100) {
      throw new Error('actions-workflow-list-incomplete');
    }
    page += 1;
  }
}

/** Read every open pull request so branch-only workflow ownership cannot be mistaken for deletion. */
export async function listAllOpenPullRequests(fetchJson, repository) {
  return listAll(fetchJson, `/repos/${repository}/pulls?state=open`, 'open-pr-list-invalid');
}

function sameRepositoryHeadSnapshot(pullRequests, repository) {
  const headShas = new Set();
  for (const pullRequest of pullRequests) {
    const headRepository = pullRequest?.head?.repo?.full_name;
    if (typeof headRepository !== 'string' || headRepository.length === 0) {
      throw new Error('open-pr-head-invalid');
    }
    if (headRepository !== repository) continue;
    const headSha = pullRequest.head.sha;
    if (typeof headSha !== 'string' || !/^[0-9a-f]{40}$/.test(headSha)) {
      throw new Error('open-pr-head-invalid');
    }
    headShas.add(headSha);
  }
  return [...headShas].sort();
}

function workflowRegistrySnapshot(records) {
  classifyWorkflowRecords(records, new Set(), new Set());
  return records
    .map((record) => JSON.stringify([record.id, record.state, record.path]))
    .sort();
}

function sameStringSnapshot(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function workflowPathsFromTree(tree, incompleteError = 'workflow-tree-incomplete') {
  if (!tree || tree.truncated !== false || !Array.isArray(tree.tree)) {
    throw new Error(incompleteError);
  }
  return tree.tree
    .filter((entry) => entry?.type === 'blob')
    .map((entry) => normalizeWorkflowPath(entry?.path))
    .filter(isRepositoryWorkflowPath);
}

/** Resolve workflow paths carried by current same-repository open PR heads. */
export async function activePullRequestWorkflowPaths(fetchJson, repository, pullRequests) {
  const headShas = sameRepositoryHeadSnapshot(pullRequests, repository);
  const workflowPaths = new Set();
  for (const headSha of headShas) {
    const tree = await fetchJson(`/repos/${repository}/git/trees/${headSha}?recursive=1`);
    for (const workflowPath of workflowPathsFromTree(tree)) workflowPaths.add(workflowPath);
  }
  return workflowPaths;
}

async function currentProtectedMainSha(fetchJson, repository) {
  const repositoryMetadata = await fetchJson(`/repos/${repository}`);
  const defaultBranch = repositoryMetadata?.default_branch;
  if (typeof defaultBranch !== 'string' || defaultBranch.length === 0) {
    throw new Error('default-branch-unavailable');
  }
  const commit = await fetchJson(`/repos/${repository}/commits/${encodeURIComponent(defaultBranch)}`);
  return commit?.sha;
}

/** Resolve workflow files from an exact, unmoved protected-main revision. */
export async function protectedWorkflowPaths(fetchJson, repository, expectedMainSha) {
  const currentMainSha = await currentProtectedMainSha(fetchJson, repository);
  if (currentMainSha !== expectedMainSha) throw new Error('protected-main-moved');
  const tree = await fetchJson(`/repos/${repository}/git/trees/${expectedMainSha}?recursive=1`);
  return new Set(workflowPathsFromTree(tree, 'protected-main-tree-incomplete'));
}

/** Build a fail-closed read-only registry audit tied to stable protected-main, registry, and open-PR snapshots. */
export async function auditActionsRegistry(fetchJson, repository, expectedMainSha) {
  const [records, mainPaths, pullRequests] = await Promise.all([
    listAllWorkflowRecords(fetchJson, repository),
    protectedWorkflowPaths(fetchJson, repository, expectedMainSha),
    listAllOpenPullRequests(fetchJson, repository),
  ]);
  const initialRegistrySnapshot = workflowRegistrySnapshot(records);
  const initialHeadSnapshot = sameRepositoryHeadSnapshot(pullRequests, repository);
  const activePrPaths = await activePullRequestWorkflowPaths(fetchJson, repository, pullRequests);

  const [refreshedRecords, refreshedPullRequests, finalMainSha] = await Promise.all([
    listAllWorkflowRecords(fetchJson, repository),
    listAllOpenPullRequests(fetchJson, repository),
    currentProtectedMainSha(fetchJson, repository),
  ]);
  if (finalMainSha !== expectedMainSha) throw new Error('protected-main-moved');

  const refreshedRegistrySnapshot = workflowRegistrySnapshot(refreshedRecords);
  if (!sameStringSnapshot(initialRegistrySnapshot, refreshedRegistrySnapshot)) {
    throw new Error('actions-workflow-snapshot-moved');
  }
  const refreshedHeadSnapshot = sameRepositoryHeadSnapshot(refreshedPullRequests, repository);
  if (!sameStringSnapshot(initialHeadSnapshot, refreshedHeadSnapshot)) {
    throw new Error('open-pr-snapshot-moved');
  }

  const classifications = classifyWorkflowRecords(records, mainPaths, activePrPaths);
  return {
    schema_version: 1,
    repository,
    protected_main_sha: expectedMainSha,
    total_records: classifications.length,
    orphaned_records: classifications.filter((entry) => entry.classification === 'orphaned-deleted'),
    unresolved_records: classifications.filter((entry) => entry.classification === 'unresolved'),
    classifications,
  };
}