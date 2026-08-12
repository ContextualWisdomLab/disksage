import path from 'node:path';

export const REPOSITORY_WORKFLOW_PREFIX = '.github/workflows/';

/** Normalize a GitHub Actions workflow path without changing its case authority. */
export function normalizeWorkflowPath(rawPath) {
  if (typeof rawPath !== 'string' || rawPath.length === 0) return null;
  const slashNormalized = rawPath.replaceAll('\\', '/');
  const withoutDotPrefix = slashNormalized.replace(/^(?:\.\/)+/, '');
  const normalized = path.posix.normalize(withoutDotPrefix);
  if (normalized === '.' || normalized.startsWith('../') || normalized.startsWith('/')) {
    return null;
  }
  return normalized;
}

/** Classify one Actions registry record against workflow paths on exact protected main. */
export function classifyWorkflowRecord(record, protectedWorkflowPaths) {
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
  return { classification: 'orphaned-deleted', path: normalizedPath, workflow_id: record?.id ?? null };
}

/** Classify a complete collection of Actions registry records. */
export function classifyWorkflowRecords(records, protectedWorkflowPaths) {
  return records.map((record) => classifyWorkflowRecord(record, protectedWorkflowPaths));
}

/** Read every page of the repository Actions workflow registry. */
export async function listAllWorkflowRecords(fetchJson, repository) {
  const records = [];
  let page = 1;
  let pageSize;
  do {
    const payload = await fetchJson(`/repos/${repository}/actions/workflows?per_page=100&page=${page}`);
    if (!payload || !Array.isArray(payload.workflows)) {
      throw new Error('actions-workflow-list-invalid');
    }
    records.push(...payload.workflows);
    pageSize = payload.workflows.length;
    page += 1;
  } while (pageSize === 100);
  return records;
}

/** Resolve workflow files from an exact, unmoved protected-main revision. */
export async function protectedWorkflowPaths(fetchJson, repository, expectedMainSha) {
  const repositoryMetadata = await fetchJson(`/repos/${repository}`);
  const defaultBranch = repositoryMetadata?.default_branch;
  if (typeof defaultBranch !== 'string' || defaultBranch.length === 0) {
    throw new Error('default-branch-unavailable');
  }
  const commit = await fetchJson(`/repos/${repository}/commits/${encodeURIComponent(defaultBranch)}`);
  if (commit?.sha !== expectedMainSha) {
    throw new Error('protected-main-moved');
  }
  const tree = await fetchJson(`/repos/${repository}/git/trees/${expectedMainSha}?recursive=1`);
  if (!tree || tree.truncated === true || !Array.isArray(tree.tree)) {
    throw new Error('protected-main-tree-incomplete');
  }
  return new Set(
    tree.tree
      .filter((entry) => entry?.type === 'blob')
      .map((entry) => normalizeWorkflowPath(entry?.path))
      .filter((workflowPath) =>
        workflowPath &&
        workflowPath.startsWith(REPOSITORY_WORKFLOW_PREFIX) &&
        /\.ya?ml$/.test(workflowPath),
      ),
  );
}

/** Build a fail-closed read-only registry audit tied to one protected-main SHA. */
export async function auditActionsRegistry(fetchJson, repository, expectedMainSha) {
  const [records, mainPaths] = await Promise.all([
    listAllWorkflowRecords(fetchJson, repository),
    protectedWorkflowPaths(fetchJson, repository, expectedMainSha),
  ]);
  const classifications = classifyWorkflowRecords(records, mainPaths);
  return {
    schema_version: 1,
    repository,
    protected_main_sha: expectedMainSha,
    total_records: classifications.length,
    orphaned_records: classifications.filter((entry) => entry.classification === 'orphaned-deleted'),
    classifications,
  };
}
