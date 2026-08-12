import path from 'node:path';

export const REPOSITORY_WORKFLOW_PREFIX = '.github/workflows/';

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
  const normalizedPath = normalizeWorkflowPath(record?.path);
  if (record?.state !== 'active') {
    return { classification: 'disabled', path: normalizedPath, workflow_id: record?.id ?? null };
  }
  if (!normalizedPath || !normalizedPath.startsWith(REPOSITORY_WORKFLOW_PREFIX)) {
    return { classification: 'github-dynamic', path: normalizedPath, workflow_id: record?.id ?? null };
  }
  if (protectedWorkflowPaths.has(normalizedPath)) {
    return { classification: 'present', path: normalizedPath, workflow_id: record?.id ?? null };
  }
  if (activePullRequestWorkflowPaths.has(normalizedPath)) {
    return {
      classification: 'unresolved',
      reason: 'active-pr-workflow',
      path: normalizedPath,
      workflow_id: record?.id ?? null,
    };
  }
  return { classification: 'orphaned-deleted', path: normalizedPath, workflow_id: record?.id ?? null };
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

/** Read every page of the repository Actions workflow registry. */
export async function listAllWorkflowRecords(fetchJson, repository) {
  const records = [];
  let page = 1;
  let pageSize;
  do {
    const payload = await fetchJson(`/repos/${repository}/actions/workflows?per_page=100&page=${page}`);
    if (!payload || !Array.isArray(payload.workflows)) throw new Error('actions-workflow-list-invalid');
    records.push(...payload.workflows);
    pageSize = payload.workflows.length;
    page += 1;
  } while (pageSize === 100);
  return records;
}

/** Read every open pull request so branch-only workflow ownership cannot be mistaken for deletion. */
export async function listAllOpenPullRequests(fetchJson, repository) {
  return listAll(fetchJson, `/repos/${repository}/pulls?state=open`, 'open-pr-list-invalid');
}

function workflowPathsFromTree(tree, incompleteError = 'workflow-tree-incomplete') {
  if (!tree || tree.truncated === true || !Array.isArray(tree.tree)) {
    throw new Error(incompleteError);
  }
  return tree.tree
    .filter((entry) => entry?.type === 'blob')
    .map((entry) => normalizeWorkflowPath(entry?.path))
    .filter(isRepositoryWorkflowPath);
}

/** Resolve workflow paths carried by current same-repository open PR heads. */
export async function activePullRequestWorkflowPaths(fetchJson, repository, pullRequests) {
  const headShas = new Set();
  for (const pullRequest of pullRequests) {
    const headRepository = pullRequest?.head?.repo?.full_name;
    if (headRepository !== repository) continue;
    const headSha = pullRequest?.head?.sha;
    if (typeof headSha !== 'string' || !/^[0-9a-f]{40}$/.test(headSha)) {
      throw new Error('open-pr-head-invalid');
    }
    headShas.add(headSha);
  }

  const workflowPaths = new Set();
  for (const headSha of headShas) {
    const tree = await fetchJson(`/repos/${repository}/git/trees/${headSha}?recursive=1`);
    for (const workflowPath of workflowPathsFromTree(tree)) workflowPaths.add(workflowPath);
  }
  return workflowPaths;
}

/** Resolve workflow files from an exact, unmoved protected-main revision. */
export async function protectedWorkflowPaths(fetchJson, repository, expectedMainSha) {
  const repositoryMetadata = await fetchJson(`/repos/${repository}`);
  const defaultBranch = repositoryMetadata?.default_branch;
  if (typeof defaultBranch !== 'string' || defaultBranch.length === 0) {
    throw new Error('default-branch-unavailable');
  }
  const commit = await fetchJson(`/repos/${repository}/commits/${encodeURIComponent(defaultBranch)}`);
  if (commit?.sha !== expectedMainSha) throw new Error('protected-main-moved');
  const tree = await fetchJson(`/repos/${repository}/git/trees/${expectedMainSha}?recursive=1`);
  return new Set(workflowPathsFromTree(tree, 'protected-main-tree-incomplete'));
}

/** Build a fail-closed read-only registry audit tied to one protected-main SHA and open-PR snapshot. */
export async function auditActionsRegistry(fetchJson, repository, expectedMainSha) {
  const [records, mainPaths, pullRequests] = await Promise.all([
    listAllWorkflowRecords(fetchJson, repository),
    protectedWorkflowPaths(fetchJson, repository, expectedMainSha),
    listAllOpenPullRequests(fetchJson, repository),
  ]);
  const activePrPaths = await activePullRequestWorkflowPaths(fetchJson, repository, pullRequests);
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
