<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./api";
  import {
    candidateReviewDecision,
    cloudDecisionReasonLabel,
    cloudReviewQueuePage,
    cloudReviewQueueStats,
    cloudReviewReasons,
    filterCloudReviewQueue,
    matchingReviewDecision as exactReviewDecision,
    ORGANIZATION_TENANT_AUTHORITY_ATTESTATION,
    organizationTenantAuthorityRequired,
    type CloudReviewQueueFilter,
    type CloudReviewQueueSort,
  } from "./cloudReviewQueue";
  import {
    boundedCloudArchiveErrorMessage,
    isCloudCopyCancelled,
  } from "./cloudArchiveErrorFeedback";
  import { fmtBytes } from "./fmt";
  import IcloudLocalEviction from "./IcloudLocalEviction.svelte";

  const RECONCILIATION_INTERVAL_MS = 60_000;
  // fileproviderctl can spend tens of seconds inside the system provider database while iCloud is
  // already unhealthy. Back off automatic probes so DiskSage does not add another hot reader.
  const ICLOUD_HEALTH_BLOCKED_RETRY_INTERVAL_MS = 5 * 60_000;
  const PROVIDER_GLOBAL_SYNC_BLOCKED_RETRY_INTERVAL_MS = 5 * 60_000;
  const PROVIDER_STALL_WARNING_MS = 15 * 60_000;
  const PROVIDER_ADMISSION_BLOCKERS = new Set([
    "icloud-new-copy-admission-blocked",
    "provider-global-sync-blocked",
    "provider-global-sync-evidence-unavailable",
  ]);
  const PROVIDER_FINDER_COPY_BLOCKERS = new Set([
    "provider-global-sync-transfer-active",
    "provider-global-sync-reconciliation-pending",
    "provider-global-sync-temporarily-disconnected",
    "provider-global-sync-server-unreachable",
    "provider-global-sync-local-disk-full",
    "provider-global-sync-item-not-found",
    "provider-global-sync-error",
    "provider-global-sync-probe-timeout",
  ]);

  function hasProviderAdmissionBlocker(notices: readonly string[]): boolean {
    return notices.some((notice) => PROVIDER_ADMISSION_BLOCKERS.has(notice));
  }

  function canCancelFinderCopyForProviderGlobalSync(
    sync: api.ProviderGlobalSyncReport | null,
  ): boolean {
    return sync?.blockers.some((blocker) => PROVIDER_FINDER_COPY_BLOCKERS.has(blocker)) ?? false;
  }

  function hasIncompleteSourceScan(notices: readonly string[]): boolean {
    return notices.includes("source-scan-incomplete");
  }

  function hasLocalEvidencePersistenceFailure(notices: readonly string[]): boolean {
    return notices.includes("local-volume-evidence-persistence-failed");
  }

  function hasRuntimeEvidencePersistenceFailure(notices: readonly string[]): boolean {
    return notices.includes("provider-client-runtime-evidence-persistence-failed");
  }

  function hasIcloudHealthEvidencePersistenceFailure(notices: readonly string[]): boolean {
    return notices.includes("icloud-sync-health-evidence-persistence-failed");
  }

  function localPressureLabel(pressure: api.LocalVolumePressure): string {
    return {
      normal: "정상",
      elevated: "상승",
      high: "높음",
      critical: "위험",
    }[pressure];
  }

  function providerProgressPercent(value: number | null | undefined): string | null {
    return value == null ? null : `${(value / 10_000).toFixed(2)}%`;
  }

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let roots: api.CloudRoot[] = $state([]);
  let rootIssues: api.CloudRootDiscoveryIssue[] = $state([]);
  let connections: api.OAuthConnection[] = $state([]);
  let reviewDecisions: api.CloudReviewDecision[] = $state([]);
  let reviewRationales: Record<string, string> = $state({});
  let reviewTenantAuthorities: Record<string, boolean> = $state({});
  let copyConfirmations: Record<string, string> = $state({});
  let copyRationales: Record<string, string> = $state({});
  let selectedRoot = $state("");
  let minSizeMib = $state(256);
  let minAgeDays = $state(90);
  let busy = $state(false);
  let loadError = $state("");
  let report: api.CloudPlanReport | null = $state(null);
  let copyingFingerprint = $state("");
  let nativeCopyActive = $state(false);
  let cancellingCopy = $state(false);
  let reviewingFingerprint = $state("");
  let copied: api.CloudCopyOutput | null = $state(null);
  let attesting = $state(false);
  let attestation: api.CloudAttestationOutput | null = $state(null);
  let reconciling = $state(false);
  let reconciliation: api.CloudReceiptReconciliationOutput | null = $state(null);
  let reconciliationError = $state("");
  let icloudHealth: api.IcloudSyncHealthReport | null = $state(null);
  let icloudHealthError = $state("");
  let checkingIcloudHealth = $state(false);
  let icloudHealthNextCheckAt = 0;
  let icloudHealthBlockedSinceMs = $state(0);
  let icloudHealthFingerprint = $state("");
  let providerGlobalSync: api.ProviderGlobalSyncReport | null = $state(null);
  let providerGlobalSyncError = $state("");
  let providerGlobalSyncObservedAtMs = $state(0);
  let providerGlobalSyncBlockedSinceMs = $state(0);
  let providerGlobalSyncFingerprint = $state("");
  let providerGlobalSyncNextCheckAt = 0;
  let checkingProviderGlobalSync = $state(false);
  let recoveringProvider = $state(false);
  let providerRecovery: api.ProviderRecoveryOutput | null = $state(null);
  let cancellingFinderCopy = $state(false);
  let finderCopyCancelStatus = $state("");
  let evicting = $state(false);
  let evictionConfirmation = $state("");
  let evictionRationale = $state("");
  let eviction: api.CloudSourceEvictionOutput | null = $state(null);
  let objectId = $state("");
  let oauthClientId = $state("");
  let oauthWriteAccess = $state(false);
  let connecting = $state(false);
  let disconnecting = $state(false);
  let checkingCapacity = $state(false);
  let connectionCapacity: api.CloudCapacitySnapshot | null = $state(null);
  let connectionCapacityRoot = $state("");
  let reviewFilter: CloudReviewQueueFilter = $state("unreviewed");
  let reviewReason = $state("");
  let reviewSort: CloudReviewQueueSort = $state("bytes-desc");
  let reviewPage = $state(1);
  let reviewStats = $derived.by(() =>
    cloudReviewQueueStats(report?.candidates ?? [], reviewDecisions)
  );
  let reviewReasons = $derived.by(() => cloudReviewReasons(report?.candidates ?? []));
  let filteredReviewCandidates = $derived.by(() =>
    filterCloudReviewQueue(
      report?.candidates ?? [],
      reviewDecisions,
      reviewFilter,
      reviewReason,
      reviewSort,
    )
  );
  let reviewPageData = $derived.by(() => cloudReviewQueuePage(filteredReviewCandidates, reviewPage));

  onMount(() => {
    const reconciliationTimer = setInterval(() => {
      if (!reconciling) void reconcileCloudReceipts();
      if (!checkingIcloudHealth) void refreshIcloudHealth();
      if (!checkingProviderGlobalSync) void refreshProviderGlobalSync();
    }, RECONCILIATION_INTERVAL_MS);
    void (async () => {
      try {
        const discovery = await api.inspectCloudRoots();
        roots = discovery.roots;
        rootIssues = discovery.issues;
        connections = await api.listCloudProviderConnections();
        reviewDecisions = await api.listCloudReviewDecisions();
        selectedRoot = roots.find((root) => root.readable)?.path ?? roots[0]?.path ?? "";
        await Promise.all([
          reconcileCloudReceipts(),
          refreshIcloudHealth(),
          refreshProviderGlobalSync(),
        ]);
      } catch (e) {
        loadError = boundedCloudArchiveErrorMessage("initialize", e);
      }
    })();
    return () => clearInterval(reconciliationTimer);
  });

  async function preview() {
    if (!scannedRoot || !selectedRoot || nativeCopyActive) return;
    busy = true;
    loadError = "";
    report = null;
    copied = null;
    attestation = null;
    eviction = null;
    evictionConfirmation = "";
    evictionRationale = "";
    objectId = "";
    reviewFilter = "unreviewed";
    reviewReason = "";
    reviewSort = "bytes-desc";
    reviewPage = 1;
    copyConfirmations = {};
    copyRationales = {};
    try {
      const planned = await api.planCloudArchive(
        scannedRoot,
        selectedRoot,
        Math.max(1, Math.floor(minSizeMib)),
        Math.max(0, Math.floor(minAgeDays)),
        200,
      );
      report = planned;
      if (planned.capacity) {
        connectionCapacity = planned.capacity.snapshot;
        connectionCapacityRoot = selectedRoot;
      }
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("preview", e);
    } finally {
      busy = false;
    }
  }

  function copyEligible(candidate: api.CloudCandidate): boolean {
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    const capacityEvidenceAvailable = api.cloudCapacityAllowsCopy(report?.capacity)
      || api.cloudNativeClientCopyAllowed(report?.capacity, selectedRootDetails(), report?.notices ?? []);
    const approvalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only");
    const providerAdmissionBlocked = report
      ? hasProviderAdmissionBlocker(report.notices)
      : true;
    const icloudAdmissionBlocked = selectedRootDetails()?.provider === "icloud"
      && icloudHealth?.new_copy_admission_state !== "clear";
    const icloudPreCopyEvidenceBlocked = selectedRootDetails()?.provider === "icloud"
      && report?.pre_copy_evidence?.complete !== true;
    return candidate.blocked_reason === null
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval)
      && capacityEvidenceAvailable
      && api.localCopyHasHeadroom(report?.local_volume, candidate.bytes)
      && !providerAdmissionBlocked
      && !icloudAdmissionBlocked
      && !icloudPreCopyEvidenceBlocked
      && approvalPhrase !== null;
  }

  function nativeCopyHeadroomBlocked(candidate: api.CloudCandidate): boolean {
    return !api.localCopyHasHeadroom(report?.local_volume, candidate.bytes);
  }

  function providerApiWriteConnected(): boolean {
    const connection = connectionForSelectedRoot();
    if (!connection) return false;
    return (connection.provider === "onedrive" && connection.scope === "Files.ReadWrite offline_access")
      || (connection.provider === "google-drive"
        && connection.scope === "https://www.googleapis.com/auth/drive");
  }

  function providerApiCopyEligible(candidate: api.CloudCandidate): boolean {
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    const approvalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only");
    return selectedRootDetails()?.provider !== "icloud"
      && hasProviderAdmissionBlocker(report?.notices ?? [])
      && providerApiWriteConnected()
      && candidate.blocked_reason === null
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval)
      && api.cloudCapacityAllowsCopy(report?.capacity)
      && approvalPhrase !== null;
  }

  function adoptEligible(candidate: api.CloudCandidate): boolean {
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    const approvalPhrase = api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy");
    return candidate.blocked_reason === "destination-exists"
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval)
      && approvalPhrase !== null;
  }

  function reviewDecision(candidate: api.CloudCandidate): api.CloudReviewDecision | null {
    return candidateReviewDecision(candidate, reviewDecisions);
  }

  function matchingReviewDecision(candidate: api.CloudCandidate): api.CloudReviewDecision | null {
    return exactReviewDecision(candidate, reviewDecisions);
  }

  async function reviewCandidate(
    candidate: api.CloudCandidate,
    disposition: api.CloudReviewDisposition,
  ) {
    if (!scannedRoot || !selectedRoot || !candidate.requires_review) return;
    const rationale = (reviewRationales[candidate.metadata_fingerprint] ?? "").trim();
    if (!rationale) return;
    const tenantAuthorityRequired = organizationTenantAuthorityRequired(candidate);
    const tenantAuthorityConfirmed =
      reviewTenantAuthorities[candidate.metadata_fingerprint] ?? false;
    if (disposition === "approved" && tenantAuthorityRequired && !tenantAuthorityConfirmed) return;
    const boundRationale =
      disposition === "approved" && tenantAuthorityRequired
        ? `${ORGANIZATION_TENANT_AUTHORITY_ATTESTATION} ${rationale}`
        : rationale;
    reviewingFingerprint = candidate.metadata_fingerprint;
    loadError = "";
    try {
      const decision = await api.reviewCloudCandidate(
        scannedRoot,
        selectedRoot,
        candidate.metadata_fingerprint,
        candidate.review_fingerprint,
        disposition,
        boundRationale,
        Math.max(1, Math.floor(minSizeMib)),
        Math.max(0, Math.floor(minAgeDays)),
        200,
      );
      reviewDecisions = [
        ...reviewDecisions.filter((entry) =>
          entry.candidate_fingerprint !== decision.candidate_fingerprint
        ),
        decision,
      ];
      reviewRationales = {
        ...reviewRationales,
        [candidate.metadata_fingerprint]: "",
      };
      reviewTenantAuthorities = {
        ...reviewTenantAuthorities,
        [candidate.metadata_fingerprint]: false,
      };
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("review", e);
    } finally {
      reviewingFingerprint = "";
    }
  }

  async function copyCandidate(candidate: api.CloudCandidate) {
    if (!scannedRoot || !selectedRoot || !copyEligible(candidate)) return;
    const exactConfirmationPhrase =
      (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim();
    const approvalRationale =
      (copyRationales[candidate.metadata_fingerprint] ?? "").trim();
    const expectedApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only");
    if (!expectedApprovalPhrase
      || exactConfirmationPhrase !== expectedApprovalPhrase
      || !approvalRationale) return;
    copyingFingerprint = candidate.metadata_fingerprint;
    // Native copy runs through the cancellable helper; adoption is verification-only.
    nativeCopyActive = true;
    loadError = "";
    copied = null;
    attestation = null;
    eviction = null;
    evictionConfirmation = "";
    evictionRationale = "";
    objectId = "";
    try {
      copied = await api.copyCloudCandidate(
        scannedRoot,
        selectedRoot,
        candidate.metadata_fingerprint,
        exactConfirmationPhrase,
        approvalRationale,
        Math.max(1, Math.floor(minSizeMib)),
        Math.max(0, Math.floor(minAgeDays)),
        200,
      );
      objectId = copied.provider_object_id ?? "";
    } catch (e) {
      loadError = isCloudCopyCancelled(e)
        ? "클라우드 복사를 취소했습니다. 원본은 유지됩니다."
        : boundedCloudArchiveErrorMessage("copy", e);
    } finally {
      copyingFingerprint = "";
      nativeCopyActive = false;
    }
  }

  async function cancelCopy() {
    if (!nativeCopyActive || !copyingFingerprint || cancellingCopy) return;
    cancellingCopy = true;
    loadError = "";
    try {
      await api.cancelCloudCopy(copyingFingerprint);
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("cancel", e);
    } finally {
      cancellingCopy = false;
    }
  }

  async function copyCandidateViaProviderApi(candidate: api.CloudCandidate) {
    if (!scannedRoot || !selectedRoot || !providerApiCopyEligible(candidate)) return;
    const exactConfirmationPhrase =
      (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim();
    const approvalRationale =
      (copyRationales[candidate.metadata_fingerprint] ?? "").trim();
    const expectedApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only");
    if (!expectedApprovalPhrase
      || exactConfirmationPhrase !== expectedApprovalPhrase
      || !approvalRationale) return;
    copyingFingerprint = candidate.metadata_fingerprint;
    nativeCopyActive = false;
    loadError = "";
    copied = null;
    attestation = null;
    eviction = null;
    evictionConfirmation = "";
    evictionRationale = "";
    objectId = "";
    try {
      copied = await api.copyCloudCandidateViaProviderApi(
        scannedRoot,
        selectedRoot,
        candidate.metadata_fingerprint,
        exactConfirmationPhrase,
        approvalRationale,
        Math.max(1, Math.floor(minSizeMib)),
        Math.max(0, Math.floor(minAgeDays)),
        200,
      );
      objectId = copied.provider_object_id ?? "";
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("provider-api-copy", e);
    } finally {
      copyingFingerprint = "";
      nativeCopyActive = false;
    }
  }

  async function adoptExistingCandidate(candidate: api.CloudCandidate) {
    if (!scannedRoot || !selectedRoot || !adoptEligible(candidate)) return;
    const exactConfirmationPhrase =
      (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim();
    const approvalRationale =
      (copyRationales[candidate.metadata_fingerprint] ?? "").trim();
    const expectedApprovalPhrase = api.cloudCopyApprovalPhrase(
      candidate,
      "adopt-existing-copy",
    );
    if (!expectedApprovalPhrase
      || exactConfirmationPhrase !== expectedApprovalPhrase
      || !approvalRationale) return;
    copyingFingerprint = candidate.metadata_fingerprint;
    // Adoption verifies an existing file without a cancellable native copy helper.
    nativeCopyActive = false;
    loadError = "";
    copied = null;
    attestation = null;
    eviction = null;
    evictionConfirmation = "";
    evictionRationale = "";
    objectId = "";
    try {
      copied = await api.adoptExistingCloudCandidate(
        scannedRoot,
        selectedRoot,
        candidate.metadata_fingerprint,
        exactConfirmationPhrase,
        approvalRationale,
        Math.max(1, Math.floor(minSizeMib)),
        Math.max(0, Math.floor(minAgeDays)),
        200,
      );
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("adopt", e);
    } finally {
      copyingFingerprint = "";
      nativeCopyActive = false;
    }
  }

  async function attestCopy() {
    if (!copied) return;
    attesting = true;
    loadError = "";
    attestation = null;
    try {
      attestation = await api.attestCloudCopy(
        copied.receipt.receipt_id,
        copied.receipt.provider === "google-drive" ? objectId.trim() || null : null,
      );
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("attest", e);
    } finally {
      attesting = false;
    }
  }

  async function reconcileCloudReceipts() {
    reconciling = true;
    reconciliationError = "";
    try {
      reconciliation = await api.reconcileCloudReceipts();
    } catch (e) {
      reconciliationError = boundedCloudArchiveErrorMessage("reconcile", e);
    } finally {
      reconciling = false;
    }
  }

  async function refreshIcloudHealth(force = false) {
    const root = selectedRootDetails();
    if (!root || root.provider !== "icloud") {
      icloudHealth = null;
      icloudHealthError = "";
      icloudHealthNextCheckAt = 0;
      icloudHealthBlockedSinceMs = 0;
      icloudHealthFingerprint = "";
      return;
    }
    if (checkingIcloudHealth || (!force && Date.now() < icloudHealthNextCheckAt)) return;
    checkingIcloudHealth = true;
    icloudHealthError = "";
    try {
      const observedAtMs = Date.now();
      const next = await api.inspectIcloudNewCopyAdmission();
      const activity = next.file_provider_activity;
      const fingerprint = [
        next.new_copy_admission_state,
        next.new_copy_admission_blockers.join(","),
        activity?.no_progress_fetch_count ?? 0,
        activity?.no_progress_create_count ?? 0,
        activity?.materialization_failure_count ?? 0,
        activity?.staged_item_missing_count ?? 0,
        activity?.active_upload_count ?? 0,
        activity?.active_download_count ?? 0,
        activity?.active_upload_progress_millionths ?? "",
        activity?.active_download_progress_millionths ?? "",
        activity?.timed_out ?? false,
      ].join("|");
      const admissionClear = next.new_copy_admission_state === "clear"
        && next.new_copy_admission_blockers.length === 0;
      if (admissionClear) {
        icloudHealthBlockedSinceMs = 0;
        icloudHealthFingerprint = "";
      } else if (icloudHealthFingerprint !== fingerprint) {
        icloudHealthBlockedSinceMs = observedAtMs;
        icloudHealthFingerprint = fingerprint;
      }
      icloudHealth = next;
      icloudHealthNextCheckAt = observedAtMs
        + (admissionClear ? RECONCILIATION_INTERVAL_MS : ICLOUD_HEALTH_BLOCKED_RETRY_INTERVAL_MS);
    } catch (e) {
      icloudHealth = null;
      icloudHealthError = boundedCloudArchiveErrorMessage("icloud-health", e);
      if (icloudHealthFingerprint !== "error") {
        icloudHealthBlockedSinceMs = Date.now();
        icloudHealthFingerprint = "error";
      }
      icloudHealthNextCheckAt = Date.now() + ICLOUD_HEALTH_BLOCKED_RETRY_INTERVAL_MS;
    } finally {
      checkingIcloudHealth = false;
    }
  }

  async function cancelFinderCopy() {
    if (cancellingFinderCopy) return;
    const provider = selectedRootDetails()?.provider;
    const isIcloud = provider === "icloud";
    cancellingFinderCopy = true;
    finderCopyCancelStatus = "";
    if (isIcloud) icloudHealthError = "";
    else providerGlobalSyncError = "";
    try {
      await api.cancelFinderCopy();
      finderCopyCancelStatus = "Finder 복사 취소 요청을 보냈습니다. 상태를 다시 확인하십시오.";
      if (isIcloud) await refreshIcloudHealth(true);
      else await refreshProviderGlobalSync(true);
    } catch (e) {
      const message = boundedCloudArchiveErrorMessage("finder-copy-cancel", e);
      if (isIcloud) icloudHealthError = message;
      else providerGlobalSyncError = message;
    } finally {
      cancellingFinderCopy = false;
    }
  }

  async function refreshProviderGlobalSync(force = false) {
    const root = selectedRootDetails();
    if (!root || root.provider === "icloud") {
      providerGlobalSync = null;
      providerGlobalSyncError = "";
      providerGlobalSyncObservedAtMs = 0;
      providerGlobalSyncBlockedSinceMs = 0;
      providerGlobalSyncFingerprint = "";
      providerGlobalSyncNextCheckAt = 0;
      return;
    }
    if (checkingProviderGlobalSync || (!force && Date.now() < providerGlobalSyncNextCheckAt)) return;
    checkingProviderGlobalSync = true;
    providerGlobalSyncError = "";
    try {
      const observedAtMs = Date.now();
      const next = await api.inspectCloudProviderGlobalSync(root.path);
      const fingerprint = [
        next.provider,
        next.state,
        next.blockers.join(","),
        next.upload_progress_present,
        next.download_progress_present,
        next.pending_indexable_count !== null && next.pending_indexable_count > 0,
      ].join("|");
      if (next.blockers.length === 0) {
        providerGlobalSyncBlockedSinceMs = 0;
        providerGlobalSyncFingerprint = "";
      } else if (providerGlobalSyncFingerprint !== fingerprint) {
        providerGlobalSyncBlockedSinceMs = observedAtMs;
        providerGlobalSyncFingerprint = fingerprint;
      }
      providerGlobalSync = next;
      providerGlobalSyncObservedAtMs = observedAtMs;
      providerGlobalSyncNextCheckAt = observedAtMs
        + (next.blockers.length === 0
          ? RECONCILIATION_INTERVAL_MS
          : PROVIDER_GLOBAL_SYNC_BLOCKED_RETRY_INTERVAL_MS);
    } catch (e) {
      const observedAtMs = Date.now();
      providerGlobalSync = null;
      providerGlobalSyncError = boundedCloudArchiveErrorMessage("provider-sync", e);
      if (providerGlobalSyncFingerprint !== "error") {
        providerGlobalSyncBlockedSinceMs = observedAtMs;
        providerGlobalSyncFingerprint = "error";
      }
      providerGlobalSyncObservedAtMs = observedAtMs;
      providerGlobalSyncNextCheckAt = observedAtMs + PROVIDER_GLOBAL_SYNC_BLOCKED_RETRY_INTERVAL_MS;
    } finally {
      checkingProviderGlobalSync = false;
    }
  }

  async function recoverProviderClient() {
    const root = selectedRootDetails();
    if (!root || root.provider === "icloud") return;
    recoveringProvider = true;
    providerRecovery = null;
    providerGlobalSyncError = "";
    try {
      providerRecovery = await api.recoverCloudProviderClient(root.path);
      await refreshProviderGlobalSync(true);
    } catch (e) {
      providerGlobalSyncError = boundedCloudArchiveErrorMessage("provider-recovery", e);
    } finally {
      recoveringProvider = false;
    }
  }

  function providerSelectionChanged() {
    providerGlobalSync = null;
    providerGlobalSyncError = "";
    providerGlobalSyncObservedAtMs = 0;
    providerGlobalSyncBlockedSinceMs = 0;
    providerGlobalSyncFingerprint = "";
    providerGlobalSyncNextCheckAt = 0;
    void refreshIcloudHealth();
    void refreshProviderGlobalSync();
  }

  function sourceEvictionReady(): boolean {
    return copied !== null
      && attestation?.permit !== null
      && attestation?.permit !== undefined
      && evictionConfirmation === copied.receipt.receipt_id
      && evictionRationale.trim().length > 0
      && !evicting;
  }

  async function evictVerifiedSource() {
    if (!copied || !sourceEvictionReady()) return;
    evicting = true;
    loadError = "";
    eviction = null;
    try {
      eviction = await api.trashVerifiedCloudSource(
        copied.receipt.receipt_id,
        evictionConfirmation,
        evictionRationale.trim(),
        copied.receipt.provider === "google-drive" ? objectId.trim() || null : null,
      );
      attestation = eviction.attestation;
      evictionConfirmation = "";
      evictionRationale = "";
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("evict", e);
    } finally {
      evicting = false;
    }
  }

  function selectedRootDetails(): api.CloudRoot | null {
    return roots.find((root) => root.path === selectedRoot) ?? null;
  }

  function connectionForSelectedRoot(): api.OAuthConnection | null {
    const root = selectedRootDetails();
    if (!root) return null;
    return connections.find((connection) => api.cloudRootIdentityMatches(connection, root)) ?? null;
  }

  function capacityForSelectedRoot(): api.CloudCapacitySnapshot | null {
    return connectionCapacityRoot === selectedRoot ? connectionCapacity : null;
  }

  function capacityUnavailableLabel(reason: string | null): string {
    const labels: Record<string, string> = {
      "provider-oauth-connection-missing": "저장된 연결 설정이 없습니다.",
      "provider-oauth-connection-ambiguous": "이 루트와 일치하는 연결 설정이 여러 개입니다.",
      "provider-oauth-connection-document-invalid": "연결 설정 문서를 안전하게 읽을 수 없습니다.",
      "provider-oauth-credential-unavailable": "OS Keychain의 refresh token을 사용할 수 없습니다. 연결 해제 후 다시 연결하세요.",
      "provider-oauth-refresh-failed": "공급자 인증 갱신에 실패했습니다. 연결 해제 후 다시 동의해야 할 수 있습니다.",
      "cloud-capacity-provider-api-unavailable": "공급자 용량 API를 현재 확인할 수 없습니다.",
      "icloud-quota-api-unavailable": "iCloud는 제3자 계정 quota API를 제공하지 않습니다.",
      "icloud-native-quota-command-unavailable": "이 macOS에서 iCloud 용량 확인 명령을 사용할 수 없습니다.",
      "icloud-native-quota-command-timeout": "iCloud 용량 확인이 시간 안에 완료되지 않았습니다.",
      "icloud-native-quota-unsupported-platform": "iCloud 네이티브 용량 확인은 macOS에서만 지원됩니다.",
      "icloud-native-quota-unavailable": "macOS가 iCloud 개인 계정 잔여 용량을 확인하지 못했습니다.",
    };
    return labels[reason ?? ""] ?? "원격 용량을 확인할 수 없습니다.";
  }

  async function verifyProviderCapacity() {
    const root = selectedRootDetails();
    if (!root) return;
    checkingCapacity = true;
    loadError = "";
    try {
      connectionCapacity = await api.verifyCloudProviderCapacity(root.path);
      connectionCapacityRoot = root.path;
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("capacity", e);
    } finally {
      checkingCapacity = false;
    }
  }

  async function connectProvider() {
    const root = selectedRootDetails();
    if (!root || root.provider === "icloud" || !oauthClientId.trim()) return;
    connecting = true;
    loadError = "";
    try {
      const connection = await api.connectCloudProvider(
        root.path,
        oauthClientId.trim(),
        oauthWriteAccess,
      );
      connections = [
        ...connections.filter((entry) => entry.connection_id !== connection.connection_id),
        connection,
      ];
      oauthClientId = "";
      connectionCapacity = await api.verifyCloudProviderCapacity(root.path);
      connectionCapacityRoot = root.path;
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("connect", e);
    } finally {
      connecting = false;
    }
  }

  async function disconnectProvider() {
    const root = selectedRootDetails();
    const connection = connectionForSelectedRoot();
    if (!root || !connection) return;
    disconnecting = true;
    loadError = "";
    try {
      await api.disconnectCloudProvider(root.path);
      connections = connections.filter((entry) => entry.connection_id !== connection.connection_id);
      connectionCapacity = null;
      connectionCapacityRoot = "";
    } catch (e) {
      loadError = boundedCloudArchiveErrorMessage("disconnect", e);
    } finally {
      disconnecting = false;
    }
  }

  function productionDate(ms: number): string {
    return new Date(ms).toLocaleDateString();
  }

  function evidenceObservedAt(ms: number): string {
    return Number.isFinite(ms) && ms > 0
      ? new Date(ms).toLocaleString()
      : "확인 시각 미상";
  }

  function syncStateLabel(state: api.ProviderSyncState | undefined): string {
    const labels: Record<api.ProviderSyncState, string> = {
      complete: "공급자 동기화 완료",
      "pending-upload": "로컬 최신본이지만 공급자 업로드 대기 중",
      "not-ubiquitous": "iCloud 관리 대상 아님",
      "not-local-current": "로컬 최신본 아님",
      uploading: "공급자 업로드 중",
      "excluded-from-sync": "공급자 동기화 제외됨",
      "sync-paused": "공급자 동기화 일시중지됨",
      "remote-unavailable": "원격 객체를 확인할 수 없음",
      "content-mismatch": "원격 콘텐츠가 로컬 복사본과 다름",
      unknown: "공급자 상태 미확인",
    };
    return labels[state ?? "unknown"] ?? labels.unknown;
  }

  function icloudBlockerLabel(blocker: string): string {
    const labels: Record<string, string> = {
      "icloud-sync-health-evidence-incomplete": "iCloud 동기화 증거가 불완전함",
      "icloud-upload-queue-nonempty": "iCloud 업로드 대기열이 남아 있음",
      "icloud-upload-in-flight": "iCloud 업로드가 진행 중임",
      "icloud-upload-blocked-on-sync-up": "iCloud sync-up 대기 항목이 있음",
      "icloud-upload-out-of-quota": "iCloud 용량 부족 항목이 있음",
      "icloud-upload-queue-state-unclassified": "분류되지 않은 iCloud 대기 상태가 있음",
      "icloud-local-sync-item-error-present": "iCloud 로컬 동기화 오류가 있음",
      "icloud-native-sync-up-pending": "macOS iCloud sync-up이 아직 끝나지 않음",
      "icloud-native-sync-down-pending": "macOS iCloud sync-down이 아직 끝나지 않음",
      "icloud-native-status-evidence-incomplete": "macOS iCloud 상태 증거가 불완전함",
      "icloud-native-status-command-timeout": "macOS iCloud 상태 확인이 시간 초과되어 복사를 보류함",
      "icloud-file-provider-no-progress": "File Provider fetch/create 요청이 진행률 없이 정지함",
      "icloud-file-provider-materialization-failed": "File Provider 파일 materialization이 실패함(staged item 없음)",
      "icloud-file-provider-item-locked": "File Provider 항목이 전파 잠금 상태임",
      "icloud-file-provider-stalled": "File Provider 오래된 오류로 전송이 정지된 상태임",
      "icloud-file-provider-filename-excluded": "iCloud가 파일 이름 때문에 동기화에서 제외한 항목이 있음",
      "icloud-file-provider-root-excluded": "iCloud가 동기화 루트에서 제외한 항목이 있음",
      "icloud-file-provider-transfer-active": "File Provider 기존 upload/download가 진행 중임",
      "icloud-file-provider-dump-timeout": "File Provider 상태 확인이 시간 초과됨",
      "icloud-file-provider-dump-output-truncated": "File Provider 상태 증거가 잘려 불완전함",
      "icloud-file-provider-evidence-unavailable": "File Provider 상태 증거를 확인할 수 없음",
      "icloud-item-error-octagon-not-signed-in": "iCloud 계정 인증이 필요함",
      "icloud-item-error-older-than-24h": "iCloud 동기화 오류가 24시간 이상 지속됨",
    };
    return labels[blocker] ?? blocker;
  }

  function providerGlobalSyncBlockerLabel(blocker: string): string {
    const labels: Record<string, string> = {
      "provider-global-sync-transfer-active": "전역 파일 전송이 진행 중임",
      "provider-global-sync-indexing-pending": "공급자 인덱싱이 끝나지 않음",
      "provider-global-sync-reconciliation-pending": "공급자 reconciliation 대기 항목이 있음",
      "provider-global-sync-filename-too-long": "파일명 제한 오류가 있음",
      "provider-global-sync-temporarily-disconnected": "공급자가 일시적으로 연결 해제됨",
      "provider-global-sync-server-unreachable": "공급자 서버에 연결할 수 없음",
      "provider-global-sync-local-disk-full": "로컬 디스크 공간 부족으로 공급자 작업이 실패함",
      "provider-global-sync-item-not-found": "공급자가 요청한 항목을 찾지 못함",
      "provider-global-sync-error": "공급자 전역 동기화 오류가 있음",
      "provider-global-sync-probe-timeout": "공급자 동기화 상태 확인이 시간 초과됨",
    };
    return labels[blocker] ?? blocker;
  }

  function providerGlobalSyncNextAction(report: api.ProviderGlobalSyncReport): string {
    if (report.state === "clear" && report.blockers.length === 0) {
      return "동기화 대기열이 비었습니다. 옮길 파일을 선택해 검사를 계속하십시오.";
    }
    if (report.probe_receipt?.next_action === "keep-local-and-rescan") {
      return "로컬 파일을 그대로 두고 공급자 동기화가 끝난 뒤 다시 검사하십시오.";
    }
    return "로컬 파일을 그대로 두고 공급자 앱의 동기화 상태를 확인한 뒤 다시 검사하십시오.";
  }

  function duration(ms: number): string {
    const totalMinutes = Math.floor(ms / 60_000);
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return hours > 0 ? `${hours}시간 ${minutes}분` : `${minutes}분`;
  }

  function accountScopeLabel(scope: api.CloudAccountScope): string {
    return {
      personal: "개인",
      organization: "조직",
      shared: "공유",
      unknown: "범위 미확인",
    }[scope];
  }
</script>

<section>
  <h2>클라우드 오프로드 <span class="dry">DRY-RUN</span></h2>
  <p class="muted">
    iCloud Drive·OneDrive·Google Drive의 로컬 루트를 탐지하고, 파일 내부 메타데이터를 우선하여 생산 시점과 원래 상대 경로를 보존하는 이동 계획만 만듭니다.
  </p>

  {#if roots.length === 0}
    <p class="warning">탐지된 클라우드 루트가 없습니다.</p>
  {:else}
    <div class="controls">
      <label>
        대상
        <select bind:value={selectedRoot} onchange={providerSelectionChanged} disabled={busy || nativeCopyActive}>
          {#each roots as root (root.id)}
            <option value={root.path}>
              {root.label} · {accountScopeLabel(root.account_scope)}{root.readable ? "" : " · 접근 불가·진단만 가능"}
            </option>
          {/each}
        </select>
      </label>
      <label>
        최소 크기(MiB)
        <input type="number" min="1" step="1" bind:value={minSizeMib} disabled={busy} />
      </label>
      <label>
        마지막 수정 후 최소 일수
        <input type="number" min="0" step="1" bind:value={minAgeDays} disabled={busy} />
      </label>
      <button onclick={preview} disabled={busy || nativeCopyActive || !scannedRoot || !selectedRoot || !selectedRootDetails()?.readable}>
        {busy ? "계획 중…" : "오프로드 후보 미리보기"}
      </button>
      <button onclick={reconcileCloudReceipts} disabled={reconciling || busy}>
        {reconciling ? "기존 보관 기록 확인 중…" : "기존 보관 기록 다시 확인"}
      </button>
      <button onclick={() => refreshIcloudHealth(true)} disabled={checkingIcloudHealth || busy}>
        {checkingIcloudHealth ? "iCloud 상태 확인 중…" : "iCloud 상태 즉시 재확인"}
      </button>
      <span class="muted">화면이 열려 있는 동안 파일을 변경하지 않고 보관 상태를 갱신합니다. iCloud 확인이 끝나지 않으면 5분 간격으로 다시 확인합니다.</span>
    </div>
    {#if selectedRootDetails() && !selectedRootDetails()?.readable}
      <p class="warning">
        이 클라우드 위치를 현재 읽을 수 없습니다. 클라우드 앱을 다시 연 뒤 상태를 재확인하십시오.
        다시 읽을 수 있을 때까지 새 복사나 원본 정리를 시작하지 않습니다.
      </p>
    {/if}
    {#if reconciliation}
      <div class="receipt-reconciliation" aria-live="polite">
        <strong>재시작 후 영수증 재검증</strong>
        <span class="context">
          {reconciliation.receipts_seen}개 확인 · {reconciliation.attested_count}개 provider 증거 갱신 ·
          {reconciliation.pending_count}개 업로드 대기 · {reconciliation.error_count}개 확인 실패
          {#if reconciliation.incomplete_reconciliation} · {reconciliation.unprocessed_count}개 미처리{/if}
        </span>
        {#if reconciliation.entries.length === 0}
          <p class="muted">저장된 cloud receipt가 없습니다.</p>
        {:else}
          {#each reconciliation.entries as entry}
            <p class:warning={entry.error !== null || entry.blockers.length > 0}>
              영수증 {entry.receipt_id ?? "무효"} · {entry.provider ?? "미확인"} ·
              Goal {entry.goal_status ?? "미확인"} ({entry.goal_state ?? "미확인"}) ·
              동기화 {syncStateLabel(entry.provider_sync_state ?? undefined)}
              {#if entry.error} · {entry.error}{/if}
              {#if entry.blockers.length > 0} · 차단: {entry.blockers.join(", ")}{/if}
            </p>
          {/each}
        {/if}
        <p class="muted">이 작업은 provider 증거와 동적 ADR/Goal만 갱신하며 클라우드 쓰기·원본 삭제는 수행하지 않습니다.</p>
      </div>
    {/if}
    {#if reconciliationError}<p class="error" role="alert">{reconciliationError}</p>{/if}
    {#if icloudHealth}
      <div class="receipt-reconciliation" aria-live="polite">
        <strong>iCloud 새 복사 admission</strong>
        <span class="context">
          {icloudHealth.new_copy_admission_state === "clear" ? "새 복사 허용 가능" : "새 복사 차단"} ·
          대기 {icloudHealth.upload_queue.scheduled_waiting_count}개 ·
          진행 {icloudHealth.upload_queue.scheduled_active_count}개 ·
          sync-up 차단 {icloudHealth.upload_queue.blocked_on_sync_up_count}개 ·
          오류 {icloudHealth.upload_queue.item_error_count}개
          {#if icloudHealth.file_provider_activity}
            · File Provider 무진행 fetch {icloudHealth.file_provider_activity.no_progress_fetch_count}개 / create {icloudHealth.file_provider_activity.no_progress_create_count}개 ·
            materialization 실패 {icloudHealth.file_provider_activity.materialization_failure_count}개 / staged item 없음 {icloudHealth.file_provider_activity.staged_item_missing_count}개 ·
            활성 upload {icloudHealth.file_provider_activity.active_upload_count}개 / download {icloudHealth.file_provider_activity.active_download_count}개
            {#if providerProgressPercent(icloudHealth.file_provider_activity.active_upload_progress_millionths)}
              · upload 진행률 {providerProgressPercent(icloudHealth.file_provider_activity.active_upload_progress_millionths)}
            {/if}
            {#if providerProgressPercent(icloudHealth.file_provider_activity.active_download_progress_millionths)}
              · download 진행률 {providerProgressPercent(icloudHealth.file_provider_activity.active_download_progress_millionths)}
            {/if}
          {/if}
        </span>
        <p class="muted">마지막 증거 확인: {evidenceObservedAt(icloudHealth.observed_at_ms)}</p>
        {#if icloudHealthBlockedSinceMs > 0}
          <p class="muted">
            동일 차단 지속: {duration(Math.max(0, icloudHealth.observed_at_ms - icloudHealthBlockedSinceMs))}
          </p>
        {/if}
        {#if hasIcloudHealthEvidencePersistenceFailure(icloudHealth.notices)}
          <p class="warning">
            iCloud 동기화 요약 증거를 저장하지 못했습니다. 이번 관찰값은 표시하되 장기 비교에는 사용하지 않으며,
            복사·원본 정리 판정은 현재 증거가 다시 저장될 때까지 보수적으로 유지합니다.
          </p>
        {/if}
        {#if icloudHealth.new_copy_admission_blockers.length > 0}
          <p class="warning">
            차단 사유:
            {icloudHealth.new_copy_admission_blockers.map(icloudBlockerLabel).join(", ")}
          </p>
          {#if icloudHealth.file_provider_activity && (
            icloudHealth.file_provider_activity.no_progress_fetch_count > 0
            || icloudHealth.file_provider_activity.no_progress_create_count > 0
            || icloudHealth.file_provider_activity.materialization_failure_count > 0
            || icloudHealth.file_provider_activity.staged_item_missing_count > 0
            || icloudHealth.file_provider_activity.timed_out
            || icloudHealth.file_provider_activity.active_upload_count > 0
            || icloudHealth.file_provider_activity.active_download_count > 0
            || icloudHealth.new_copy_admission_blockers.includes("icloud-file-provider-item-locked")
            || icloudHealth.new_copy_admission_blockers.includes("icloud-file-provider-stalled")
          )}
            <button onclick={cancelFinderCopy} disabled={cancellingFinderCopy || checkingIcloudHealth}>
              {cancellingFinderCopy ? "Finder 복사 취소 요청 중…" : "Finder 복사 취소 요청"}
            </button>
            {#if finderCopyCancelStatus}<p class="muted">{finderCopyCancelStatus}</p>{/if}
          {/if}
          {#if icloudHealth.file_provider_activity && (icloudHealth.file_provider_activity.no_progress_fetch_count > 0 || icloudHealth.file_provider_activity.no_progress_create_count > 0)}
            <p class="warning">
              Finder가 “복사 준비 중”에서 멈춘 동안 File Provider의 no-progress 요청이 함께 관찰되었습니다. Finder에 남은 복사 대기는 취소하고,
              File Provider 상태가 정상으로 관찰된 뒤 DiskSage에서 새 계획을 다시 실행해야 합니다.
            </p>
          {/if}
          {#if icloudHealth.file_provider_activity && (icloudHealth.file_provider_activity.materialization_failure_count > 0 || icloudHealth.file_provider_activity.staged_item_missing_count > 0)}
            <p class="warning">
              File Provider가 파일 materialization에 실패했거나 staged item을 잃었습니다. 현재 복사는 완료로 간주하지 않으며,
              클라우드 앱이 정상화될 때까지 새 복사나 원본 정리를 시작하지 않습니다.
            </p>
          {/if}
          {#if icloudHealth.new_copy_admission_blockers.includes("icloud-file-provider-item-locked")}
            <p class="warning">
              File Provider 항목의 전파 잠금 상태가 Finder 복사 준비 지연과 함께 관찰되었습니다. Finder의 대기 작업을 취소하고,
              상태가 정상화된 뒤 DiskSage에서 새 복사를 다시 시작하십시오.
            </p>
          {/if}
          {#if icloudHealth.new_copy_admission_blockers.includes("icloud-file-provider-stalled")}
            <p class="warning">
              File Provider 큐에서 15분 이상 묵은 fetch/create 오류가 관찰되었습니다. Finder의 “복사 준비 중” 작업을 취소하고,
              상태가 정상화된 뒤 DiskSage에서 새 복사를 다시 시작하십시오.
            </p>
          {/if}
          {#if icloudHealth.file_provider_activity?.timed_out}
            <p class="warning">
              File Provider 상태 확인이 제한시간을 넘었습니다. Finder에 남은 복사 대기를 취소하고,
              DiskSage에서 상태를 다시 확인해 완료 안내가 표시될 때만 새 복사를 시작하십시오.
            </p>
          {/if}
          {#if icloudHealth.file_provider_activity && (icloudHealth.file_provider_activity.active_upload_count > 0 || icloudHealth.file_provider_activity.active_download_count > 0)}
            <p class="warning">
              iCloud에 기존 전송이 진행 중입니다. 기존 전송이 끝나고 완료 안내가 표시될 때까지
              Finder 복사와 원본 정리를 진행하지 않습니다.
            </p>
          {/if}
          {#if icloudHealthBlockedSinceMs > 0 && icloudHealth.observed_at_ms - icloudHealthBlockedSinceMs >= PROVIDER_STALL_WARNING_MS}
            <p class="warning">
              동일한 iCloud 차단 상태가 15분 이상 지속되었습니다. Finder에 남은 복사 대기를 취소하고,
              iCloud 상태 확인이 끝날 때까지 새 복사나 원본 정리를 시작하지 마십시오.
            </p>
          {/if}
        {:else}
          <p class="capacity-ok">iCloud 대기 작업이 없습니다. 옮길 파일을 선택하면 해당 파일의 보관 상태를 한 번 더 확인합니다.</p>
        {/if}
        {#if typeof icloudHealth.managed_database_allocated_bytes === "number"}
          <p class="warning">
            macOS 관리 iCloud 동기화 DB가 {fmtBytes(icloudHealth.managed_database_allocated_bytes)}를 사용 중입니다.
            DiskSage는 이 시스템 관리 데이터를 삭제하지 않습니다.
          </p>
        {/if}
        {#if icloudHealth.notices.some((notice) => notice.startsWith("icloud-item-error-"))}
          <p class="warning">
            동기화 진단:
            {icloudHealth.notices
              .filter((notice) => notice.startsWith("icloud-item-error-"))
              .map(icloudBlockerLabel)
              .join(", ")}
          </p>
        {/if}
        <p class="muted">읽기 전용 로컬 증거이며, 원격 용량·개별 파일 업로드 완료·원본 삭제 권한을 대신 증명하지 않습니다.</p>
      </div>
    {/if}
    {#if icloudHealthError}
      <p class="error" role="alert">iCloud 상태 확인: {icloudHealthError}</p>
      <p class="warning">
        iCloud File Provider 증거를 확인하지 못했습니다. Finder에 남은 복사 대기를 취소하고,
        로컬 여유공간을 확보한 뒤 DiskSage에서 상태를 다시 확인하십시오.
      </p>
    {/if}
    {#if providerGlobalSync}
      <div class="receipt-reconciliation" aria-live="polite">
        <strong>{providerGlobalSync.provider} 동기화 확인</strong>
        <p class:warning={providerGlobalSync.blockers.length > 0}>
          {providerGlobalSyncNextAction(providerGlobalSync)}
        </p>
        <span class="context">마지막 확인 {evidenceObservedAt(providerGlobalSyncObservedAtMs)} · {providerGlobalSync.blockers.length === 0 ? "1분" : "5분"} 후 자동 재확인</span>
        {#if providerGlobalSync.blockers.length > 0}
          <p class="warning">
            차단 사유: {providerGlobalSync.blockers.map(providerGlobalSyncBlockerLabel).join(", ")}
          </p>
          {#if providerGlobalSyncBlockedSinceMs > 0 && providerGlobalSyncObservedAtMs - providerGlobalSyncBlockedSinceMs >= PROVIDER_STALL_WARNING_MS}
            <p class="warning">
              동기화 확인이 15분 이상 완료되지 않았습니다. Finder에 남은 복사 대기를 취소하고,
              공급자 앱을 다시 연 뒤 DiskSage가 완료 상태를 확인할 때까지 새 복사나 원본 정리를 시작하지 마십시오.
            </p>
          {/if}
          {#if selectedRootDetails()?.provider !== "icloud"}
            <button onclick={recoverProviderClient} disabled={recoveringProvider || checkingProviderGlobalSync}>
              {recoveringProvider ? "공급자 앱 재기동 중…" : "공급자 앱 재기동 후 상태 재확인"}
            </button>
            {#if canCancelFinderCopyForProviderGlobalSync(providerGlobalSync)}
              <button onclick={cancelFinderCopy} disabled={cancellingFinderCopy || checkingProviderGlobalSync}>
                {cancellingFinderCopy ? "Finder 복사 취소 요청 중…" : "Finder 복사 취소 요청"}
              </button>
              {#if finderCopyCancelStatus}<p class="muted">{finderCopyCancelStatus}</p>{/if}
            {/if}
          {/if}
        {:else}
          <p class="capacity-ok">옮길 파일을 선택하면 해당 파일의 보관 상태를 한 번 더 확인합니다.</p>
        {/if}
        <details>
          <summary>감사 세부 정보</summary>
          <p class="muted">
            상태 {providerGlobalSync.state} · 증거 {providerGlobalSync.evidence_kind} ·
            {providerGlobalSync.evidence_complete ? "완료" : "불완전"}
          </p>
          {#if providerGlobalSync.probe_receipt}
            <p class="muted">결과 {providerGlobalSync.probe_receipt.outcome} · {providerGlobalSync.probe_receipt.audit_reason_codes.join(", ")}</p>
          {/if}
        </details>
        {#if providerRecovery}
          <p class:warning={providerRecovery.blockers.length > 0} class="muted">
            앱 종료·재기동 요청 완료 · 재관찰
            {providerRecovery.post_runtime_observed === true ? "확인됨" : "아직 확인되지 않음"}
            {#if providerRecovery.blockers.length > 0} · {providerRecovery.blockers.join(", ")}{/if}
          </p>
        {/if}
        <p class="muted">확인이 끝나기 전에는 로컬 파일을 그대로 유지합니다.</p>
      </div>
    {/if}
    {#if providerGlobalSyncError}
      <p class="error" role="alert">클라우드 동기화 상태를 확인하지 못했습니다.</p>
      <p class="warning">
        로컬 파일을 그대로 두십시오. Finder에 남은 복사 대기를 취소하고 공급자 앱을 다시 연 뒤 재검사하십시오.
      </p>
      <details><summary>감사 세부 정보</summary><p class="muted">{providerGlobalSyncError}</p></details>
    {/if}
    {#if roots.some((root) => !root.readable)}
      <p class="warning">
        접근 불가 클라우드 루트는 선택에서 제외했습니다. macOS 개인정보 보호 권한을 허용한 뒤 목록을 다시 불러오세요.
      </p>
    {/if}
    {#if rootIssues.length > 0}
      <p class="warning">
        클라우드 루트 탐지 문제 {rootIssues.length}건: {rootIssues.map((issue) => `${issue.provider ?? "file-provider"}/${issue.account_scope}/${issue.reason}`).join(", ")}
      </p>
    {/if}
    {#if selectedRootDetails()?.provider === "icloud"}
      <div class="oauth-panel">
        <strong>macOS iCloud 계정 용량 증거</strong>
        <button onclick={verifyProviderCapacity} disabled={checkingCapacity}>
          {checkingCapacity ? "iCloud 계정 확인 중…" : "iCloud 원격 잔여 용량 검증"}
        </button>
        {#if capacityForSelectedRoot()?.evidence_kind === "provider-native-status"}
          <p class="capacity-ok">
            Apple 네이티브 계정 상태 확인 완료
            · 원격 잔여 {fmtBytes(capacityForSelectedRoot()?.remaining_bytes ?? 0)}
          </p>
        {:else if capacityForSelectedRoot()}
          <p class="warning">
            {capacityUnavailableLabel(capacityForSelectedRoot()?.unavailable_reason ?? null)}
          </p>
        {:else}
          <p class="muted">관리자 권한이나 OAuth 없이 macOS의 읽기 전용 iCloud 계정 상태를 사용합니다.</p>
        {/if}
        {#key selectedRoot}
          <IcloudLocalEviction cloudRoot={selectedRoot} />
        {/key}
      </div>
    {:else if selectedRootDetails()}
      <div class="oauth-panel">
        {#if connectionForSelectedRoot()}
          <strong>{providerApiWriteConnected() ? "OAuth 업로드 연결" : "읽기 전용 OAuth descriptor 발견"}</strong>
          <span class="context">범위: {connectionForSelectedRoot()?.scope}</span>
          <button
            onclick={verifyProviderCapacity}
            disabled={checkingCapacity || disconnecting || connecting}
          >
            {checkingCapacity ? "Keychain·원격 API 확인 중…" : "재시작 후 연결·원격 용량 검증"}
          </button>
          <button onclick={disconnectProvider} disabled={disconnecting || connecting || checkingCapacity}>
            {disconnecting ? "연결 해제 중…" : "보안 저장소 연결 해제"}
          </button>
          {#if capacityForSelectedRoot()?.evidence_kind === "provider-api"}
            <p class="capacity-ok">
              Keychain 인증 갱신과 공급자 API 확인 완료
              {#if capacityForSelectedRoot()?.remaining_bytes !== null}
                · 원격 잔여 {fmtBytes(capacityForSelectedRoot()?.remaining_bytes ?? 0)}
              {:else}
                · 공급자 무제한 계정
              {/if}
            </p>
          {:else if capacityForSelectedRoot()}
            <p class="warning">
              {capacityUnavailableLabel(capacityForSelectedRoot()?.unavailable_reason ?? null)}
            </p>
          {:else}
            <p class="muted">
              descriptor만 확인했습니다. 재시작 후 Keychain 자격 증명과 실제 공급자 API는 아직 검증하지 않았습니다.
            </p>
          {/if}
        {:else}
          <label>
            {selectedRootDetails()?.provider === "onedrive" ? "Microsoft Desktop OAuth Client ID" : "Google Desktop OAuth Client ID"}
            <input
              class="client-id"
              type="text"
              bind:value={oauthClientId}
              autocomplete="off"
              spellcheck="false"
              disabled={connecting}
            />
          </label>
          <label>
            <input type="checkbox" bind:checked={oauthWriteAccess} disabled={connecting} />
            File Provider 장애 시 API로 파일 업로드할 권한도 요청
          </label>
          <button onclick={connectProvider} disabled={connecting || !oauthClientId.trim()}>
            {connecting ? "브라우저 동의 대기 중…" : "시스템 브라우저로 OAuth 연결"}
          </button>
          <p class="muted">
            Client ID는 비밀키가 아닙니다. PKCE와 임의 loopback 포트를 사용하고 refresh token만 OS 보안 저장소에 보관합니다.
          </p>
          {#if selectedRootDetails()?.provider === "onedrive"}
            <p class="muted">Microsoft Entra 앱은 Mobile/Desktop public client로 만들고 loopback redirect URI <code>http://localhost</code>를 등록해야 합니다. 실행 시 임의 포트를 붙이며 IPv4·IPv6 loopback만 수신합니다.</p>
          {/if}
          {#if selectedRootDetails()?.provider === "google-drive"}
            <p class="warning">Google OAuth Client 유형은 Desktop app이어야 합니다. 업로드 fallback을 선택하면 Drive 파일 쓰기 권한 동의가 필요합니다. 동의하지 않으면 읽기 전용 attestation만 사용합니다.</p>
          {/if}
        {/if}
      </div>
    {/if}
  {/if}

  {#if !scannedRoot}<p class="muted">먼저 스캔을 완료하세요.</p>{/if}
  {#if loadError}<p class="error">{loadError}</p>{/if}

  {#if nativeCopyActive && copyingFingerprint}
    <div class="copy-progress" aria-live="polite">
      <span>진행 중인 클라우드 복사는 후보 상태가 바뀌어도 취소할 수 있습니다.</span>
      <button
        type="button"
        onclick={cancelCopy}
        disabled={cancellingCopy}
        aria-label="진행 중인 DiskSage 클라우드 복사 취소"
      >
        {cancellingCopy ? "복사 취소 요청 중…" : "진행 중인 복사 취소"}
      </button>
    </div>
  {/if}

  {#if report}
    <div class="summary">
      {report.candidates.length}개 후보 · 총 {fmtBytes(report.candidate_bytes)} ·
      충돌 제외 잠재 회수 {fmtBytes(report.potentially_reclaimable_bytes)}
    </div>
    {#if hasProviderAdmissionBlocker(report.notices)}
      <p class="warning">
        현재 공급자 전역 동기화 증거가 불완전하거나 전송 중입니다. 새 copy-only 버튼은 비활성화되며,
        상태가 해소된 뒤 다시 계획해야 합니다. 기존 복사본 채택·per-item attestation은 별도 경로로 동작합니다.
      </p>
    {/if}
    {#if hasIncompleteSourceScan(report.notices)}
      <p class="warning">
        원본 스캔이 제한 시간 또는 항목 수에 도달해 부분 결과만 수집되었습니다. 이 계획은 복사·원본 제거에 사용할 수 없으며,
        스캔 범위를 줄이거나 조건을 높여 전체 스캔을 다시 실행해야 합니다.
      </p>
    {/if}
    {#if report.local_volume}
      <p class:warning={report.local_volume.pressure !== "normal"}>
        원본 볼륨 압력: {localPressureLabel(report.local_volume.pressure)} · 사용 가능
        {fmtBytes(report.local_volume.available_bytes)}
        ({(report.local_volume.available_basis_points / 100).toFixed(2)}%)
      </p>
      {#if hasLocalEvidencePersistenceFailure(report.notices)}
        <p class="warning">
          디스크 용량 증거를 저장하지 못했습니다. 이번 계획은 실행할 수 있지만, 사고 비교용 용량 이력은 남지 않습니다.
        </p>
      {/if}
      {#if report.candidates.some(nativeCopyHeadroomBlocked)}
        <p class="warning">
          네이티브 File Provider 복사는 후보 크기와 {fmtBytes(api.LOCAL_COPY_RESERVE_BYTES)} 여유공간을 함께 확보해야 합니다.
          현재 여유공간이 부족한 후보는 버튼을 비활성화합니다. 명시적 OAuth 공급자 API 업로드는 별도 경로입니다.
        </p>
      {/if}
    {/if}
    {#if hasRuntimeEvidencePersistenceFailure(report.notices)}
      <p class="warning">
        공급자 클라이언트 관찰 증거를 저장하지 못했습니다. 프로세스 이력은 남지 않으며, 복사·동기화 판정은 현재 관찰값으로만 제한됩니다.
      </p>
    {/if}
    {#if hasIcloudHealthEvidencePersistenceFailure(report.notices)}
      <p class="warning">
        iCloud 동기화 요약 증거를 저장하지 못했습니다. 이번 계획은 표시하되 장기 비교용 provider 이력은 남지 않습니다.
        상태가 정상으로 관찰된 뒤 다시 계획하십시오.
      </p>
    {/if}
    {#if report.pre_copy_evidence && !report.pre_copy_evidence.complete}
      <p class="warning">
        사전 복사 증거 시점/무결성 cohort가 완성되지 않아 새 복사를 차단합니다:
        {report.pre_copy_evidence.blockers.join(", ")}
      </p>
    {/if}
    {#if report.capacity}
      {#if report.capacity.can_fit === true}
        <p class="capacity-ok">
          원격 계정 용량 확인됨 · 요청 {fmtBytes(report.capacity.requested_bytes)} + 보존 여유
          {fmtBytes(report.capacity.reserve_bytes)}
          {#if report.capacity.snapshot.remaining_bytes !== null}
            · 원격 잔여 {fmtBytes(report.capacity.snapshot.remaining_bytes)}
          {:else}
            · 공급자 무제한 계정
          {/if}
        </p>
      {:else if report.capacity.can_fit === false}
        <p class="warning">
          원격 용량 gate 실패: {report.capacity.blockers.join(", ")}
        </p>
      {:else}
        <p class="warning">
          원격 quota를 검증할 수 없음: {report.capacity.snapshot.unavailable_reason ?? "cloud-capacity-unavailable"}.
          {#if api.cloudNativeClientCopyAllowed(report.capacity, selectedRootDetails(), report.notices)}
            개인 native-client 모드: 실행 중인 OneDrive·Google Drive 앱으로 copy-only를 진행하고, 개별 sync 증거 전에는 원본을 보존합니다.
          {:else}
            OneDrive·Google Drive는 읽기 전용 OAuth 연결 후 다시 계획해야 복사할 수 있습니다.
          {/if}
          iCloud는 macOS 네이티브 계정 상태 확인 후 다시 계획해야 복사할 수 있습니다.
        </p>
      {/if}
    {/if}
    {#if report.exact_duplicates.candidate_count > 0}
      <p class="warning">
        정확 중복 {report.exact_duplicates.candidate_count.toLocaleString()}개 ·
        {report.exact_duplicates.cluster_count.toLocaleString()}개 콘텐츠 클러스터 ·
        대표본 외 중복 경로 {fmtBytes(report.exact_duplicates.redundant_bytes)}.
        동일 크기 후보만 로컬 SHA-256·BLAKE3로 확인했으며, 대표 lineage를 선택하기 전에는 자동 복사하지 않습니다.
        정본 추천은 내장 생산일·신뢰도·내장 메타데이터를 먼저 비교하고, 다운로드·압축해제
        출처 맥락과 격리·복사본 경로를 별도 보조 기준으로 사용합니다. 추천
        {report.exact_duplicates.clusters.length.toLocaleString()}건 모두 사람 확인이 필요하며, 낮은 신뢰도 추천은
        {report.exact_duplicates.clusters.filter((cluster) => cluster.recommendation_confidence === "low").length.toLocaleString()}건입니다.
      </p>
    {/if}
    <p class="warning">
      생산일 우선순위는 내장 메타데이터 → 명시적 파일명 날짜 → 파일시스템 생성 → 수정 시각입니다. 파일명 날짜와 파일시스템 시각은 저신뢰 잠정값이며, 현재 메타데이터와 목적지에 결박된 명시적 승인 없이는 복사할 수 없습니다. 이미 존재하는 클라우드 파일은 전체 콘텐츠 해시가 모두 같을 때만 채택합니다. 원본은 공급자 증거를 실행 순간 다시 확인하고 전체 영수증 ID와 사유를 직접 승인한 경우에만 휴지통으로 이동하며, 휴지통은 비우지 않습니다.
    </p>
    {#if copied}
      <div class="receipt">
        <strong>{copied.goal_status === "blocked" ? "복사 완료 · 공급자 확인 차단" : copied.action === "adopt-existing-copy" ? "기존 클라우드 복사본 검증·채택 완료" : "검증 복사 완료"} · 원본 보존됨</strong>
        <div class="context">영수증 {copied.receipt.receipt_id} · {fmtBytes(copied.receipt.bytes)}</div>
        <div class="path">{copied.receipt.destination}</div>
        <p class="muted">Goal: {copied.goal_state} · 상태: {copied.goal_status ?? "미확인"} · 동적 ADR: {copied.adr_path ?? "실패"} · 동적 Goal: {copied.goal_path ?? "실패"}</p>
        {#each copied.projection_warnings as warning}
          <p class="warning">동적 ADR/Goal 투영 경고: {warning}</p>
        {/each}
        {#if copied.receipt.provider === "google-drive"}
          <div class="provider-auth">
            <label>
              Google Drive file ID (선택)
              <input type="text" bind:value={objectId} autocomplete="off" disabled={attesting} />
            </label>
          </div>
          <p class="muted">먼저 macOS File Provider의 업로드·최신 버전 메타데이터를 확인합니다. file ID를 입력하면 네이티브 증거가 불완전할 때 OAuth API로 SHA-256과 부모 폴더 체인을 My Drive 루트까지 두 차례 검증합니다. 영수증 목적지와 정확히 일치하고 검증 중 변경되지 않은 경우에만 원본 제거 허가를 생성합니다. 공유 드라이브는 아직 실패 폐쇄합니다.</p>
          <p class="muted">API 보완 시 access token은 OS 보안 저장소의 refresh token으로 Rust 내부에서 한 번만 갱신하며 UI·설정·영수증에 노출하지 않습니다.</p>
        {:else if copied.receipt.provider === "onedrive"}
          <p class="muted">macOS File Provider 증거가 불완전하면 OAuth 연결을 사용해 영수증의 OneDrive 상대 경로를 직접 조회하고 QuickXorHash를 검증합니다. 임의 item ID는 받지 않습니다.</p>
        {/if}
        <button
          onclick={attestCopy}
          disabled={attesting}
        >
          {attesting ? "검증 중…" : "클라우드 업로드 상태·콘텐츠 확인"}
        </button>
        {#if attestation}
          <p class:warning={attestation.goal_state !== "eviction-ready"} class:safe={attestation.goal_state === "eviction-ready"}>
            Goal: {attestation.goal_state} · 상태 {attestation.goal_status ?? "미확인"} ·
            {syncStateLabel(attestation.evidence.sync_state)}
          </p>
          {#if attestation.assessment.state === "overdue"}
            <p class="warning">
              공급자 확인이 {Math.floor(attestation.assessment.pending_age_ms / 3_600_000)}시간째 완료되지 않았습니다. 원본은 계속 보존하며 iCloud/File Provider 상태를 점검해야 합니다.
            </p>
          {:else if attestation.assessment.state === "pending"}
            <p class="muted">
              공급자 확인 대기 {Math.floor(attestation.assessment.pending_age_ms / 60_000)}분. 완료 전에는 원본을 제거하지 않습니다.
            </p>
          {/if}
          {#if attestation.permit}
            {#if eviction}
              <p class="safe">원본을 운영체제 휴지통으로 이동했습니다. 클라우드 목적지는 유지되며 휴지통은 비우지 않았습니다.</p>
              <p class="muted">사람 승인 {eviction.approval.approval_id} · 완료 {eviction.eviction.completion_id}</p>
              <p class="muted">변경 불가 승인 기록: {eviction.approval_path}</p>
              {#if eviction.adr_path || eviction.goal_path}
                <p class="muted">동적 ADR: {eviction.adr_path || "실패"} · 동적 Goal: {eviction.goal_path || "실패"}</p>
              {/if}
              {#each eviction.projection_warnings as warning}
                <p class="warning">동적 ADR/Goal 투영 경고: {warning}</p>
              {/each}
            {:else}
              <p class="safe">업로드 상태와 복사 콘텐츠 검증 완료. 로컬 제거 허가 증거가 생성되었지만 파일은 아직 그대로 보존됩니다.</p>
              <div class="eviction-controls">
                <p class="warning">
                  아래 전체 영수증 ID를 직접 입력하고 이 파일만 휴지통으로 옮기는 사유를 남겨야 합니다. 실행 시 공급자 상태와 열린 파일·프로세스 참조를 다시 확인하며, 달라지면 중단합니다.
                </p>
                <div class="context">확인할 영수증 ID: {copied.receipt.receipt_id}</div>
                <label>
                  전체 영수증 ID 확인
                  <input
                    class="receipt-confirmation"
                    type="text"
                    bind:value={evictionConfirmation}
                    autocomplete="off"
                    spellcheck="false"
                    disabled={evicting}
                  />
                </label>
                <label>
                  원본 휴지통 이동 사유
                  <textarea
                    bind:value={evictionRationale}
                    maxlength="1000"
                    disabled={evicting}
                    placeholder="예: 공급자 업로드·콘텐츠 검증 완료 후 이 영수증의 로컬 원본만 휴지통으로 이동"
                  ></textarea>
                </label>
                <button onclick={evictVerifiedSource} disabled={!sourceEvictionReady()}>
                  {evicting ? "공급자·사용 중 상태 재검증 후 이동 중…" : "검증을 다시 수행하고 원본을 휴지통으로 이동"}
                </button>
              </div>
            {/if}
          {:else}
            <p class="warning">아직 제거 불가: {attestation.blockers.join(", ")}</p>
          {/if}
          <p class="muted">변경 불가 공급자 증거 기록: {attestation.evidence_path}</p>
          <p class="muted">동적 ADR: {attestation.adr_path ?? "실패"} · 동적 Goal: {attestation.goal_path ?? "실패"}</p>
          {#each attestation.projection_warnings as warning}
            <p class="warning">동적 ADR/Goal 투영 경고: {warning}</p>
          {/each}
        {/if}
      </div>
    {/if}
    {#if report.candidates.length === 0}
      <p class="muted">현재 크기·경과일·지원 파일 유형 조건에 맞는 후보가 없습니다.</p>
    {:else}
      <div class="review-queue" aria-label="클라우드 메타데이터 검토 큐">
        <div class="review-progress" aria-live="polite">
          <strong>
            검토 진행 {reviewStats.reviewed.toLocaleString()} / {reviewStats.reviewable.toLocaleString()}개
          </strong>
          <progress
            max={Math.max(1, reviewStats.reviewable)}
            value={reviewStats.reviewed}
            aria-label={`메타데이터 검토 ${reviewStats.reviewed}개 완료, ${reviewStats.unreviewed}개 남음`}
          ></progress>
          <span>
            남음 {reviewStats.unreviewed.toLocaleString()}개 · {fmtBytes(reviewStats.unreviewedBytes)}
          </span>
        </div>
        <div class="review-counts">
          <span>승인 {reviewStats.approved.toLocaleString()}</span>
          <span>보류 {reviewStats.held.toLocaleString()}</span>
          <span>차단 {reviewStats.blocked.toLocaleString()} · {fmtBytes(reviewStats.blockedBytes)}</span>
          <span>자동 진행 가능 {reviewStats.ready.toLocaleString()}</span>
        </div>
        <div class="review-filters">
          <label>
            상태
            <select bind:value={reviewFilter} onchange={() => reviewPage = 1}>
              <option value="unreviewed">미검토</option>
              <option value="approved">승인</option>
              <option value="held">보류</option>
              <option value="blocked">차단</option>
              <option value="ready">자동 진행 가능</option>
              <option value="all">전체</option>
            </select>
          </label>
          <label>
            검토 사유
            <select bind:value={reviewReason} onchange={() => reviewPage = 1}>
              <option value="">모든 사유</option>
              {#each reviewReasons as reason}
                <option value={reason}>{cloudDecisionReasonLabel(reason)}</option>
              {/each}
            </select>
          </label>
          <label>
            정렬
            <select bind:value={reviewSort} onchange={() => reviewPage = 1}>
              <option value="bytes-desc">큰 파일 먼저</option>
              <option value="production-asc">생산일 오래된 순</option>
              <option value="production-desc">생산일 최신 순</option>
            </select>
          </label>
        </div>
        <div class="review-pagination" aria-live="polite">
          <button
            onclick={() => reviewPage = Math.max(1, reviewPageData.page - 1)}
            disabled={reviewPageData.page <= 1}
          >이전 20개</button>
          <span>
            {reviewPageData.startIndex.toLocaleString()}–{reviewPageData.endIndex.toLocaleString()} /
            {reviewPageData.totalItems.toLocaleString()}개 · {reviewPageData.page} / {reviewPageData.totalPages}쪽
          </span>
          <button
            onclick={() => reviewPage = Math.min(reviewPageData.totalPages, reviewPageData.page + 1)}
            disabled={reviewPageData.page >= reviewPageData.totalPages}
          >다음 20개</button>
        </div>
      </div>
      {#if reviewPageData.items.length === 0}
        <p class="muted">현재 상태·사유 필터에 맞는 후보가 없습니다.</p>
      {:else}
      <ul class="candidates">
        {#each reviewPageData.items as candidate (candidate.metadata_fingerprint)}
          <li class:blocked={candidate.blocked_reason !== null} class:adoptable={adoptEligible(candidate)}>
            <div class="line">
              <strong>{fmtBytes(candidate.bytes)}</strong>
              <span>{candidate.kind}</span>
              <span>생산 {productionDate(candidate.production_time_ms)}</span>
              <span>근거 {candidate.production_time_source} ({candidate.production_time_confidence})</span>
              <span>수정 후 {candidate.age_days.toLocaleString()}일</span>
              {#if candidate.requires_review}<em>맥락/민감정보 검토 필요</em>{/if}
              {#if candidate.blocked_reason}<em>{cloudDecisionReasonLabel(candidate.blocked_reason)}</em>{/if}
            </div>
            <div class="path" title={candidate.src}>{candidate.src}</div>
            {#if candidate.content_title}
              <div class="metadata">내장 제목: {candidate.content_title}</div>
            {/if}
            {#if candidate.content_authors.length > 0}
              <div class="metadata">작성자/아티스트: {candidate.content_authors.join(", ")}</div>
            {/if}
            {#if candidate.content_context.length > 0}
              <div class="metadata">보존 맥락: {candidate.content_context.join(" · ")}</div>
            {/if}
            {#if candidate.duration_ms !== null}
              <div class="metadata">재생 시간: {duration(candidate.duration_ms)}</div>
            {/if}
            {#if candidate.dataset_profile}
              <div class="dataset-profile">
                <strong>
                  데이터 메타데이터: {candidate.dataset_profile.format.toUpperCase()} ·
                  표본 {candidate.dataset_profile.sampled_rows.toLocaleString()}행 ·
                  {candidate.dataset_profile.columns.length.toLocaleString()}열
                </strong>
                {#if candidate.dataset_profile.worksheet_names.length > 0}
                  <div class="metadata">
                    시트 {candidate.dataset_profile.sampled_worksheets.toLocaleString()}개:
                    {candidate.dataset_profile.worksheet_names.join(", ")}
                  </div>
                {/if}
                <div class="metadata">
                  {candidate.dataset_profile.profile_complete ? "스키마 표본 완료" : "스키마 표본 불완전·검토 필요"}
                  {candidate.dataset_profile.sample_truncated ? " · 제한 범위까지만 읽음" : ""}
                </div>
                {#if candidate.dataset_profile.columns.length > 0}
                  <ul class="schema-columns">
                    {#each candidate.dataset_profile.columns as column}
                      <li>
                        {column.name}: {column.inferred_type} · 관측 {column.observed_values.toLocaleString()} ·
                        결측 {column.missing_values.toLocaleString()}
                        {#if column.sensitive_name}<em>민감 컬럼명 징후</em>{/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
                {#if candidate.dataset_profile.quality_warnings.length > 0}
                  <div class="context">데이터 품질 경고: {candidate.dataset_profile.quality_warnings.join(", ")}</div>
                {/if}
                <div class="context">셀 값은 저장하거나 표시하지 않습니다.</div>
              </div>
            {/if}
            <div class="arrow">→ {candidate.dst}</div>
            <div class="context">
              맥락: {candidate.source_context} · 대상 계정: {accountScopeLabel(candidate.destination_account_scope)} · lineage: {candidate.metadata_fingerprint.slice(0, 12)}
            </div>
            <details class="lineage">
              <summary>Lineage 연결관계</summary>
              <ol>
                <li>
                  원본 Entity · <code>{candidate.metadata_fingerprint}</code> · {candidate.source_context}
                </li>
                <li>
                  생산 시각 Activity · {candidate.production_time_source} · 신뢰도 {candidate.production_time_confidence}
                </li>
                <li>아카이브 Entity · {candidate.kind} · {fmtBytes(candidate.bytes)}</li>
                <li>공급자 Entity · {candidate.provider} · {candidate.destination_account_scope} · {candidate.dst}</li>
              </ol>
              <p class="context">
                검증 복사 영수증 → provider attestation → Goal/ADR → 명시적 휴지통 이동 순서로 연결됩니다.
                {candidate.blocked_reason
                  ? ` 현재 관계 차단: ${cloudDecisionReasonLabel(candidate.blocked_reason)}.`
                  : " 아직 영수증이 없어 provider item과 eviction permit은 확정되지 않았습니다."}
              </p>
            </details>
            {#if candidate.requires_review}
              <div class="review-controls">
                {#if matchingReviewDecision(candidate)?.disposition === "approved"}
                  <strong class="approved">현재 메타데이터 증거 검토 승인됨</strong>
                {:else if matchingReviewDecision(candidate)?.disposition === "held"}
                  <strong class="held">현재 메타데이터 증거 보류됨</strong>
                {:else if reviewDecision(candidate)}
                  <strong class="held">메타데이터 증거가 바뀌어 이전 결정이 만료됨</strong>
                {:else}
                  <span class="context">아래 증거를 확인한 뒤 승인 또는 보류하세요.</span>
                {/if}
                {#if matchingReviewDecision(candidate)}
                  <span class="context">
                    검토자: {matchingReviewDecision(candidate)?.reviewed_by ?? "legacy-local-operator"} ·
                    근거: {matchingReviewDecision(candidate)?.rationale ?? "legacy decision"}
                  </span>
                {/if}
                <label class="review-rationale">
                  새 승인·보류 근거 (민감한 셀 값이나 문서 본문은 입력하지 마세요)
                  <textarea
                    maxlength="1000"
                    value={reviewRationales[candidate.metadata_fingerprint] ?? ""}
                    oninput={(event) => {
                      reviewRationales = {
                        ...reviewRationales,
                        [candidate.metadata_fingerprint]: event.currentTarget.value,
                      };
                    }}
                    disabled={reviewingFingerprint !== ""}
                  ></textarea>
                </label>
                {#if organizationTenantAuthorityRequired(candidate)}
                  <label>
                    <input
                      type="checkbox"
                      checked={reviewTenantAuthorities[candidate.metadata_fingerprint] ?? false}
                      onchange={(event) => {
                        reviewTenantAuthorities = {
                          ...reviewTenantAuthorities,
                          [candidate.metadata_fingerprint]: event.currentTarget.checked,
                        };
                      }}
                      disabled={reviewingFingerprint !== ""}
                    />
                    이 조직 테넌트가 해당 민감 자료를 보관할 권한이 있음을 확인했습니다.
                  </label>
                {/if}
                <button
                  onclick={() => reviewCandidate(candidate, "approved")}
                  disabled={reviewingFingerprint !== ""
                    || !(reviewRationales[candidate.metadata_fingerprint] ?? "").trim()
                    || (organizationTenantAuthorityRequired(candidate)
                      && !(reviewTenantAuthorities[candidate.metadata_fingerprint] ?? false))}
                >
                  {reviewingFingerprint === candidate.metadata_fingerprint ? "기록 중…" : "메타데이터 검토 승인"}
                </button>
                <button
                  onclick={() => reviewCandidate(candidate, "held")}
                  disabled={reviewingFingerprint !== "" || !(reviewRationales[candidate.metadata_fingerprint] ?? "").trim()}
                >보류</button>
              </div>
            {/if}
            {#if copyEligible(candidate) || providerApiCopyEligible(candidate)}
              {@const copyApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only")}
              <div class="copy-approval">
                <div class="context">현재 메타데이터·출발지·목적지에 결부된 문구를 정확히 입력해야 합니다.</div>
                <code>{copyApprovalPhrase ?? "현재 계획의 승인 문구를 확인할 수 없습니다."}</code>
                <label>
                  복사 승인 사유
                  <textarea
                    maxlength="1000"
                    value={copyRationales[candidate.metadata_fingerprint] ?? ""}
                    oninput={(event) => {
                      copyRationales = {
                        ...copyRationales,
                        [candidate.metadata_fingerprint]: event.currentTarget.value,
                      };
                    }}
                    disabled={copyingFingerprint !== ""}
                  ></textarea>
                </label>
                <label>
                  정확한 복사 승인 문구
                  <input
                    class="receipt-confirmation"
                    value={copyConfirmations[candidate.metadata_fingerprint] ?? ""}
                    oninput={(event) => {
                      copyConfirmations = {
                        ...copyConfirmations,
                        [candidate.metadata_fingerprint]: event.currentTarget.value,
                      };
                    }}
                    disabled={copyingFingerprint !== ""}
                  />
                </label>
                {#if copyEligible(candidate)}
                  <button
                    class="copy"
                    onclick={() => copyCandidate(candidate)}
                    disabled={copyingFingerprint !== ""
                      || copied?.receipt.candidate_fingerprint === candidate.metadata_fingerprint
                      || !(copyRationales[candidate.metadata_fingerprint] ?? "").trim()
                      || copyApprovalPhrase === null
                      || (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim()
                        !== copyApprovalPhrase}
                >
                  {copyingFingerprint === candidate.metadata_fingerprint ? "복사·해시 검증 중…" : "원본을 유지하고 클라우드에 복사"}
                </button>
                {/if}
                {#if providerApiCopyEligible(candidate)}
                  <p class="warning">File Provider 전역 동기화가 막혀 있어, 명시적 OAuth 쓰기 연결로 공급자 API에 직접 업로드합니다. 원본은 유지되고 이후 API attestation이 필요합니다.</p>
                  <button
                    class="copy"
                    onclick={() => copyCandidateViaProviderApi(candidate)}
                    disabled={copyingFingerprint !== ""
                      || copied?.receipt.candidate_fingerprint === candidate.metadata_fingerprint
                      || !(copyRationales[candidate.metadata_fingerprint] ?? "").trim()
                      || copyApprovalPhrase === null
                      || (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim()
                        !== copyApprovalPhrase}
                  >
                    {copyingFingerprint === candidate.metadata_fingerprint ? "공급자 API 업로드 중…" : "File Provider를 우회해 공급자 API로 업로드"}
                  </button>
                {/if}
              </div>
            {/if}
            {#if adoptEligible(candidate)}
              {@const adoptApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")}
              <div class="copy-approval">
                <div class="context">기존 목적지 파일의 전체 해시 검증·채택도 정확한 별도 승인이 필요합니다.</div>
                <code>{adoptApprovalPhrase ?? "현재 계획의 채택 승인 문구를 확인할 수 없습니다."}</code>
                <label>
                  기존 복사본 채택 사유
                  <textarea
                    maxlength="1000"
                    value={copyRationales[candidate.metadata_fingerprint] ?? ""}
                    oninput={(event) => {
                      copyRationales = {
                        ...copyRationales,
                        [candidate.metadata_fingerprint]: event.currentTarget.value,
                      };
                    }}
                    disabled={copyingFingerprint !== ""}
                  ></textarea>
                </label>
                <label>
                  정확한 채택 승인 문구
                  <input
                    class="receipt-confirmation"
                    value={copyConfirmations[candidate.metadata_fingerprint] ?? ""}
                    oninput={(event) => {
                      copyConfirmations = {
                        ...copyConfirmations,
                        [candidate.metadata_fingerprint]: event.currentTarget.value,
                      };
                    }}
                    disabled={copyingFingerprint !== ""}
                  />
                </label>
                <button
                  class="copy"
                  onclick={() => adoptExistingCandidate(candidate)}
                  disabled={copyingFingerprint !== ""
                    || copied?.receipt.candidate_fingerprint === candidate.metadata_fingerprint
                    || !(copyRationales[candidate.metadata_fingerprint] ?? "").trim()
                    || adoptApprovalPhrase === null
                    || (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim()
                      !== adoptApprovalPhrase}
                >
                  {copyingFingerprint === candidate.metadata_fingerprint ? "기존 파일 전체 해시 검증 중…" : "기존 클라우드 복사본 해시 검증·채택"}
                </button>
              </div>
            {/if}
            <details>
              <summary>메타데이터 증거 {candidate.metadata_evidence.length}건</summary>
              <ul class="evidence">
                {#each candidate.metadata_evidence as evidence}
                  <li>{evidence.field}: {evidence.value} · {evidence.source} · {evidence.confidence}</li>
                {/each}
              </ul>
            </details>
            {#if candidate.review_reasons.length > 0}
              <div class="context">검토 사유: {candidate.review_reasons.map(cloudDecisionReasonLabel).join(", ")}</div>
            {/if}
          </li>
        {/each}
      </ul>
      {/if}
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.5rem; align-items: center; }
  .dry { font-size: 0.7rem; color: #fff; background: #59636e; border-radius: 8px; padding: 2px 7px; }
  .controls { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: end; }
  .provider-auth { display: flex; flex-wrap: wrap; gap: 0.75rem; margin: 0.5rem 0; }
  .oauth-panel { margin-top: 0.75rem; padding: 0.75rem; border: 1px solid #b7c6d8; border-radius: 4px; background: #f6f9fc; display: grid; gap: 0.45rem; justify-items: start; }
  label { display: grid; gap: 0.2rem; font-size: 0.8rem; color: #555; }
  select { max-width: 32rem; }
  input { width: 7rem; }
  .provider-auth input { width: min(32rem, 75vw); }
  .client-id { width: min(40rem, 80vw); }
  .summary { margin-top: 0.8rem; font-weight: 600; }
  .receipt-reconciliation { margin-top: 0.75rem; padding: 0.75rem; border: 1px solid #b7c6d8; border-radius: 4px; background: #f8fafc; display: grid; gap: 0.35rem; }
  .receipt { margin: 0.75rem 0; padding: 0.75rem; border: 1px solid #6b8e72; border-radius: 4px; background: #f5fbf6; }
  .eviction-controls { margin-top: 0.75rem; padding: 0.75rem; border: 1px solid #b78335; border-radius: 4px; background: #fffaf1; display: grid; gap: 0.55rem; }
  .eviction-controls textarea { width: min(52rem, 88vw); min-height: 3.5rem; resize: vertical; }
  .receipt-confirmation { width: min(52rem, 88vw); font-family: ui-monospace, monospace; }
  .review-controls { display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; margin: 0.5rem 0; }
  .review-rationale { flex-basis: 100%; }
  .review-rationale textarea { width: min(52rem, 88vw); min-height: 3.5rem; resize: vertical; }
  .review-queue { margin: 0.75rem 0; padding: 0.75rem; border: 1px solid #b7c6d8; border-radius: 4px; background: #f8fafc; display: grid; gap: 0.6rem; }
  .review-progress { display: flex; flex-wrap: wrap; align-items: center; gap: 0.65rem; }
  .review-progress progress { width: min(24rem, 70vw); }
  .review-counts { display: flex; flex-wrap: wrap; gap: 0.75rem; color: #59636e; font-size: 0.78rem; }
  .review-filters { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: end; }
  .review-filters select { max-width: min(30rem, 80vw); }
  .review-pagination { display: flex; flex-wrap: wrap; align-items: center; gap: 0.65rem; font-size: 0.8rem; }
  .approved { color: #25643b; }
  .held { color: #8a4b16; }
  .candidates { list-style: none; margin: 0.5rem 0; padding: 0; max-height: 34rem; overflow-y: auto; }
  .candidates li { padding: 0.6rem; border: 1px solid #e3e3e3; border-radius: 4px; margin-bottom: 0.4rem; }
  .candidates li.blocked { border-color: #b03030; background: #fff7f7; }
  .candidates li.adoptable { border-color: #6b8e72; background: #f5fbf6; }
  .line { display: flex; flex-wrap: wrap; gap: 0.6rem; font-size: 0.8rem; }
  .line em { color: #9a5b00; }
  .path, .arrow { overflow-wrap: anywhere; font-size: 0.85rem; }
  .arrow { color: #555; margin-top: 0.2rem; }
  .metadata { color: #3f5368; font-size: 0.78rem; margin-top: 0.2rem; }
  .dataset-profile { margin-top: 0.4rem; padding: 0.45rem; border: 1px solid #c8d4df; border-radius: 4px; background: #f8fafc; font-size: 0.78rem; }
  .schema-columns { margin: 0.25rem 0; padding-left: 1.2rem; max-height: 10rem; overflow-y: auto; }
  .schema-columns em { margin-left: 0.4rem; color: #9a5b00; }
  .context { color: #777; font-size: 0.75rem; margin-top: 0.2rem; }
  .copy-approval { margin-top: 0.55rem; padding: 0.55rem; border: 1px solid #c8d4df; border-radius: 4px; background: #f8fafc; display: grid; gap: 0.45rem; }
  .copy-approval code { overflow-wrap: anywhere; font-size: 0.72rem; }
  .copy-approval label { display: grid; gap: 0.2rem; font-size: 0.78rem; }
  .copy-approval textarea { width: min(52rem, 88vw); min-height: 3.5rem; resize: vertical; }
  .copy { margin-top: 0.4rem; }
  details { margin-top: 0.3rem; color: #59636e; font-size: 0.75rem; }
  .evidence { margin: 0.25rem 0 0; padding-left: 1.2rem; }
  .muted { color: #777; }
  .warning { color: #8a5700; }
  .safe { color: #276437; }
  .error { color: #b00; }
</style>
