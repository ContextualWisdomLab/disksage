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
  import { productionTimeConfidenceLabel } from "./productionTimeConfidenceLabel";
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
      "provider-oauth-connection-missing": "저장된 연결이 없습니다. 클라우드 연결을 먼저 설정하십시오.",
      "provider-oauth-connection-ambiguous": "이 클라우드 위치에 맞는 연결을 확인하지 못했습니다. 연결을 다시 설정하십시오.",
      "provider-oauth-connection-document-invalid": "클라우드 연결 정보를 읽을 수 없습니다. 연결을 다시 설정하십시오.",
      "provider-oauth-credential-unavailable": "클라우드 연결을 사용할 수 없습니다. 연결을 해제한 뒤 다시 연결하십시오.",
      "provider-oauth-refresh-failed": "클라우드 인증을 갱신하지 못했습니다. 연결을 해제한 뒤 다시 연결하십시오.",
      "cloud-capacity-provider-api-unavailable": "클라우드 저장 공간을 확인할 수 없습니다. 잠시 후 다시 시도하십시오.",
      "icloud-quota-api-unavailable": "iCloud 저장 공간을 확인할 수 없습니다. macOS 계정 상태를 다시 확인하십시오.",
      "icloud-native-quota-command-unavailable": "이 macOS에서는 iCloud 저장 공간을 확인할 수 없습니다.",
      "icloud-native-quota-command-timeout": "iCloud 용량 확인이 시간 안에 완료되지 않았습니다.",
      "icloud-native-quota-unsupported-platform": "iCloud 네이티브 용량 확인은 macOS에서만 지원됩니다.",
      "icloud-native-quota-unavailable": "macOS가 iCloud 계정의 남은 저장 공간을 확인하지 못했습니다.",
    };
    return labels[reason ?? ""] ?? "클라우드 저장 공간을 확인할 수 없습니다. 잠시 후 다시 시도하십시오.";
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
      complete: "클라우드 동기화 완료",
      "pending-upload": "로컬 최신본이며 클라우드 업로드 대기 중",
      "not-ubiquitous": "iCloud 동기화 대상 아님",
      "not-local-current": "로컬 최신본 아님",
      uploading: "클라우드 업로드 중",
      "excluded-from-sync": "클라우드 동기화에서 제외됨",
      "sync-paused": "클라우드 동기화 일시중지됨",
      "remote-unavailable": "클라우드 파일을 확인할 수 없음",
      "content-mismatch": "클라우드 파일과 로컬 복사본이 다름",
      unknown: "클라우드 상태 미확인",
    };
    return labels[state ?? "unknown"] ?? labels.unknown;
  }

  function icloudBlockerLabel(blocker: string): string {
    const labels: Record<string, string> = {
      "icloud-sync-health-evidence-incomplete": "iCloud 동기화 상태를 확인하지 못함",
      "icloud-upload-queue-nonempty": "iCloud 업로드 대기 항목이 남아 있음",
      "icloud-upload-in-flight": "iCloud 업로드가 진행 중임",
      "icloud-upload-blocked-on-sync-up": "iCloud 업로드를 기다리는 항목이 있음",
      "icloud-upload-out-of-quota": "iCloud 저장 공간이 부족함",
      "icloud-upload-queue-state-unclassified": "iCloud 업로드 상태를 확인하지 못함",
      "icloud-local-sync-item-error-present": "iCloud 동기화 오류가 있음",
      "icloud-native-sync-up-pending": "iCloud 업로드가 아직 끝나지 않음",
      "icloud-native-sync-down-pending": "iCloud 다운로드가 아직 끝나지 않음",
      "icloud-native-status-evidence-incomplete": "iCloud 상태를 확인하지 못함",
      "icloud-native-status-command-timeout": "iCloud 상태 확인이 오래 걸려 복사를 보류함",
      "icloud-file-provider-no-progress": "iCloud 파일 처리가 멈춰 있음",
      "icloud-file-provider-materialization-failed": "iCloud 파일 준비에 실패함",
      "icloud-file-provider-item-locked": "iCloud 파일 처리가 잠겨 있음",
      "icloud-file-provider-stalled": "iCloud 파일 전송이 멈춰 있음",
      "icloud-file-provider-filename-excluded": "파일 이름 때문에 iCloud 동기화에서 제외된 항목이 있음",
      "icloud-file-provider-root-excluded": "iCloud 동기화에서 제외된 항목이 있음",
      "icloud-file-provider-transfer-active": "iCloud 파일 전송이 진행 중임",
      "icloud-file-provider-dump-timeout": "iCloud 상태 확인이 오래 걸림",
      "icloud-file-provider-dump-output-truncated": "iCloud 상태를 모두 확인하지 못함",
      "icloud-file-provider-evidence-unavailable": "iCloud 상태를 확인할 수 없음",
      "icloud-item-error-octagon-not-signed-in": "iCloud 계정 로그인이 필요함",
      "icloud-item-error-older-than-24h": "iCloud 동기화 오류가 24시간 이상 지속됨",
    };
    return labels[blocker] ?? "iCloud 상태를 확인하지 못함";
  }

  function providerGlobalSyncBlockerLabel(blocker: string): string {
    const labels: Record<string, string> = {
      "provider-global-sync-transfer-active": "클라우드 파일 전송이 진행 중임",
      "provider-global-sync-indexing-pending": "클라우드 파일 확인이 끝나지 않음",
      "provider-global-sync-reconciliation-pending": "클라우드 파일 확인 대기 항목이 있음",
      "provider-global-sync-filename-too-long": "파일명 제한 오류가 있음",
      "provider-global-sync-temporarily-disconnected": "클라우드 연결이 일시적으로 끊어짐",
      "provider-global-sync-server-unreachable": "클라우드 서버에 연결할 수 없음",
      "provider-global-sync-local-disk-full": "로컬 저장 공간 부족으로 클라우드 작업이 실패함",
      "provider-global-sync-item-not-found": "클라우드에서 요청한 항목을 찾지 못함",
      "provider-global-sync-error": "클라우드 동기화 오류가 있음",
      "provider-global-sync-probe-timeout": "클라우드 동기화 상태 확인이 오래 걸림",
    };
    return labels[blocker] ?? "클라우드 상태를 확인하지 못함";
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

  function customerDecisionReasonLabel(reason: string): string {
    const label = cloudDecisionReasonLabel(reason);
    if (label === reason) return "추가 확인 필요";
    return label
      .replaceAll("공급자", "클라우드")
      .replaceAll("증거", "확인")
      .replaceAll("스키마", "파일 구조")
      .replaceAll("메타데이터", "파일 정보")
      .replaceAll("attestation", "업로드 확인");
  }
</script>

<section>
  <h2>클라우드 파일 정리 계획 <span class="dry">미리보기</span></h2>
  <p class="muted">
    iCloud Drive·OneDrive·Google Drive의 로컬 위치를 확인하고, 파일 정보를 바탕으로 생산 시점과 원래 상대 경로를 보존하는 이동 계획만 만듭니다.
  </p>

  {#if roots.length === 0}
    <p class="warning">탐지된 클라우드 루트가 없습니다. 클라우드 앱이 연결되어 있는지 확인한 뒤 새로고침하십시오.</p>
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
        {reconciling ? "이전 작업 상태 확인 중…" : "이전 작업 상태 다시 확인"}
      </button>
      <button onclick={() => refreshIcloudHealth(true)} disabled={checkingIcloudHealth || busy}>
        {checkingIcloudHealth ? "iCloud 상태 확인 중…" : "iCloud 상태 즉시 재확인"}
      </button>
      <span class="muted">화면이 열려 있는 동안 클라우드에 쓰거나 원본을 삭제하지 않고 상태만 확인합니다. iCloud 처리가 지연되면 자동 확인 간격이 최대 5분으로 늘어납니다.</span>
    </div>
    {#if selectedRootDetails() && !selectedRootDetails()?.readable}
      <p class="warning">
        이 클라우드 위치를 현재 읽을 수 없습니다. 클라우드 앱 상태 확인과 앱 재시작만 가능하며,
        위치가 다시 읽힐 때까지 복사와 원본 정리를 막습니다.
      </p>
    {/if}
    {#if reconciliation}
      <div class="receipt-reconciliation" aria-live="polite">
        <strong>이전 작업 상태 확인</strong>
        <span class="context">
          {reconciliation.receipts_seen}개 확인 · {reconciliation.attested_count}개 업로드 확인 ·
          {reconciliation.pending_count}개 업로드 대기 · {reconciliation.error_count}개 확인 실패
          {#if reconciliation.incomplete_reconciliation} · {reconciliation.unprocessed_count}개 미처리{/if}
        </span>
        {#if reconciliation.entries.length === 0}
          <p class="muted">저장된 작업이 없습니다. 새 계획을 실행해 상태를 확인하십시오.</p>
        {:else}
          {#each reconciliation.entries as entry}
            <p class:warning={entry.error !== null || entry.blockers.length > 0}>
              {entry.error !== null || entry.blockers.length > 0
                ? "확인이 필요한 작업이 있습니다. 클라우드 상태를 다시 확인하십시오."
                : `업로드 상태: ${syncStateLabel(entry.provider_sync_state ?? undefined)}`}
            </p>
          {/each}
        {/if}
        <p class="muted">이 확인은 클라우드에 쓰거나 원본을 삭제하지 않습니다.</p>
      </div>
    {/if}
    {#if reconciliationError}<p class="error" role="alert">{reconciliationError}</p>{/if}
    {#if icloudHealth}
      <div class="receipt-reconciliation" aria-live="polite">
        <strong>iCloud 새 복사 상태</strong>
        <span class="context">
          {icloudHealth.new_copy_admission_state === "clear" ? "새 복사 허용 가능" : "새 복사 차단"} ·
          대기 {icloudHealth.upload_queue.scheduled_waiting_count}개 ·
          진행 {icloudHealth.upload_queue.scheduled_active_count}개 ·
          업로드 대기 차단 {icloudHealth.upload_queue.blocked_on_sync_up_count}개 ·
          오류 {icloudHealth.upload_queue.item_error_count}개
          {#if icloudHealth.file_provider_activity}
            · 클라우드 앱 처리 대기 {icloudHealth.file_provider_activity.no_progress_fetch_count + icloudHealth.file_provider_activity.no_progress_create_count}개 ·
            파일 준비 실패 {icloudHealth.file_provider_activity.materialization_failure_count}개 ·
            진행 중인 업로드 {icloudHealth.file_provider_activity.active_upload_count}개 / 다운로드 {icloudHealth.file_provider_activity.active_download_count}개
            {#if providerProgressPercent(icloudHealth.file_provider_activity.active_upload_progress_millionths)}
              · 업로드 진행률 {providerProgressPercent(icloudHealth.file_provider_activity.active_upload_progress_millionths)}
            {/if}
            {#if providerProgressPercent(icloudHealth.file_provider_activity.active_download_progress_millionths)}
              · 다운로드 진행률 {providerProgressPercent(icloudHealth.file_provider_activity.active_download_progress_millionths)}
            {/if}
          {/if}
        </span>
        <p class="muted">마지막 확인: {evidenceObservedAt(icloudHealth.observed_at_ms)}</p>
        {#if icloudHealthBlockedSinceMs > 0}
          <p class="muted">
            같은 상태가 지속된 시간: {duration(Math.max(0, icloudHealth.observed_at_ms - icloudHealthBlockedSinceMs))}
          </p>
        {/if}
        {#if hasIcloudHealthEvidencePersistenceFailure(icloudHealth.notices)}
          <p class="warning">
            iCloud 동기화 상태를 저장하지 못했습니다. 잠시 후 상태를 다시 확인한 뒤 복사 계획을 다시 실행하십시오.
          </p>
        {/if}
        {#if icloudHealth.new_copy_admission_blockers.length > 0}
          <p class="warning">
            지금 진행할 수 없는 이유:
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
              Finder의 복사 준비가 멈춰 있습니다. Finder에 남은 복사 대기를 취소하고,
              상태가 정상화된 뒤 DiskSage에서 새 계획을 다시 실행하십시오.
            </p>
          {/if}
          {#if icloudHealth.file_provider_activity && (icloudHealth.file_provider_activity.materialization_failure_count > 0 || icloudHealth.file_provider_activity.staged_item_missing_count > 0)}
            <p class="warning">
              클라우드 파일을 준비하지 못했습니다. 현재 복사는 완료로 간주하지 않으며,
              상태가 정상화될 때까지 새 복사와 원본 정리를 막습니다. 상태가 정상화된 뒤 새 계획을 다시 실행하십시오.
            </p>
          {/if}
          {#if icloudHealth.new_copy_admission_blockers.includes("icloud-file-provider-item-locked")}
            <p class="warning">
              클라우드 파일 처리가 잠겨 있습니다. Finder의 대기 작업을 취소하고,
              상태가 정상화된 뒤 DiskSage에서 새 복사를 다시 시작하십시오.
            </p>
          {/if}
          {#if icloudHealth.new_copy_admission_blockers.includes("icloud-file-provider-stalled")}
            <p class="warning">
              클라우드 파일 처리가 15분 이상 멈춰 있습니다. Finder의 “복사 준비 중” 작업을 취소하고,
              상태가 정상화된 뒤 DiskSage에서 새 복사를 다시 시작하십시오.
            </p>
          {/if}
          {#if icloudHealth.file_provider_activity?.timed_out}
            <p class="warning">
              클라우드 상태 확인이 오래 걸립니다. Finder에 남은 복사 대기를 취소하고,
              DiskSage에서 상태를 다시 확인한 뒤 새 복사를 시작하십시오.
            </p>
          {/if}
          {#if icloudHealth.file_provider_activity && (icloudHealth.file_provider_activity.active_upload_count > 0 || icloudHealth.file_provider_activity.active_download_count > 0)}
            <p class="warning">
              iCloud에서 다른 전송이 진행 중입니다. 업로드와 다운로드가 끝날 때까지
              Finder 복사와 원본 정리를 진행하지 않습니다.
            </p>
          {/if}
          {#if icloudHealthBlockedSinceMs > 0 && icloudHealth.observed_at_ms - icloudHealthBlockedSinceMs >= PROVIDER_STALL_WARNING_MS}
            <p class="warning">
              iCloud 처리가 15분 이상 멈춰 있습니다. Finder에 남은 복사 대기를 취소하고,
              iCloud 상태가 정상화될 때까지 새 복사와 원본 정리를 시작하지 마십시오.
            </p>
          {/if}
        {:else}
          <p class="capacity-ok">iCloud 업로드 대기 항목이 없습니다. 파일별 상태를 확인한 뒤 진행하십시오.</p>
        {/if}
        {#if typeof icloudHealth.managed_database_allocated_bytes === "number"}
          <p class="warning">
            macOS가 관리하는 iCloud 데이터가 {fmtBytes(icloudHealth.managed_database_allocated_bytes)}를 사용 중입니다.
            DiskSage는 이 시스템 데이터를 삭제하지 않습니다.
          </p>
        {/if}
        {#if icloudHealth.notices.some((notice) => notice.startsWith("icloud-item-error-"))}
          <p class="warning">
            동기화 확인:
            {icloudHealth.notices
              .filter((notice) => notice.startsWith("icloud-item-error-"))
              .map(icloudBlockerLabel)
              .join(", ")}
          </p>
        {/if}
        <p class="muted">이 확인은 로컬 상태만 보여주며, 클라우드 업로드 완료나 원본 삭제 권한을 보장하지 않습니다.</p>
      </div>
    {/if}
    {#if icloudHealthError}
      <p class="error" role="alert">iCloud 상태를 확인하지 못했습니다.</p>
      <p class="warning">
        Finder에 남은 복사 대기를 취소하고,
        로컬 여유공간을 확보한 뒤 DiskSage에서 상태를 다시 확인하십시오.
      </p>
    {/if}
    {#if providerGlobalSync}
      <div class="receipt-reconciliation" aria-live="polite">
        <strong>클라우드 전체 동기화 상태</strong>
        <span class="context">
          {providerGlobalSync.state === "clear" && providerGlobalSync.blockers.length === 0 ? "새 복사 가능" : "새 복사 차단"} ·
          업로드 전송 {providerGlobalSync.upload_progress_present ? "진행 중" : "없음"} ·
          다운로드 전송 {providerGlobalSync.download_progress_present ? "진행 중" : "없음"}
          {#if providerGlobalSync.pending_indexable_count !== null}
            · 확인 대기 {providerGlobalSync.pending_indexable_count}개
          {/if}
          · 마지막 확인 {evidenceObservedAt(providerGlobalSyncObservedAtMs)} ·
          {providerGlobalSync.blockers.length === 0 ? "1분" : "5분"} 후 자동 재확인
          {#if providerGlobalSyncBlockedSinceMs > 0}
            · 같은 상태가 지속된 시간 {duration(Math.max(0, providerGlobalSyncObservedAtMs - providerGlobalSyncBlockedSinceMs))}
          {/if}
        </span>
        {#if providerGlobalSync.blockers.length > 0}
          <p class="warning">
            지금 진행할 수 없는 이유: {providerGlobalSync.blockers.map(providerGlobalSyncBlockerLabel).join(", ")}
          </p>
          {#if providerGlobalSyncBlockedSinceMs > 0 && providerGlobalSyncObservedAtMs - providerGlobalSyncBlockedSinceMs >= PROVIDER_STALL_WARNING_MS}
            <p class="warning">
              클라우드 처리가 15분 이상 멈춰 있습니다. Finder에 남은 복사 대기를 취소하고,
              클라우드 앱을 재기동한 뒤 상태가 정상화될 때까지 새 복사와 원본 정리를 시작하지 마십시오.
            </p>
          {/if}
          {#if selectedRootDetails()?.provider !== "icloud"}
            <button onclick={recoverProviderClient} disabled={recoveringProvider || checkingProviderGlobalSync}>
              {recoveringProvider ? "클라우드 앱 재기동 중…" : "클라우드 앱 재기동 후 상태 재확인"}
            </button>
            {#if canCancelFinderCopyForProviderGlobalSync(providerGlobalSync)}
              <button onclick={cancelFinderCopy} disabled={cancellingFinderCopy || checkingProviderGlobalSync}>
                {cancellingFinderCopy ? "Finder 복사 취소 요청 중…" : "Finder 복사 취소 요청"}
              </button>
              {#if finderCopyCancelStatus}<p class="muted">{finderCopyCancelStatus}</p>{/if}
            {/if}
          {/if}
        {:else}
          <p class="capacity-ok">클라우드 전체 동기화 대기 항목이 없습니다. 파일별 상태를 확인한 뒤 진행하십시오.</p>
        {/if}
        {#if providerRecovery}
          <p class:warning={providerRecovery.blockers.length > 0} class="muted">
            앱 재기동 요청 완료 · 상태 재확인
            {providerRecovery.post_runtime_observed === true ? "확인됨" : "아직 확인되지 않음"}
            {#if providerRecovery.blockers.length > 0} · 추가 확인이 필요합니다.{/if}
          </p>
        {/if}
        <p class="muted">이 확인은 클라우드 상태만 보여주며, 파일 업로드 완료나 원본 삭제 권한을 보장하지 않습니다.</p>
      </div>
    {/if}
    {#if providerGlobalSyncError}
      <p class="error" role="alert">클라우드 전체 동기화 상태를 확인하지 못했습니다.</p>
      <p class="warning">
        Finder에 남은 복사 대기를 취소하고,
        클라우드 앱이 정상화될 때까지 새 복사와 원본 정리를 시작하지 마십시오.
      </p>
    {/if}
    {#if roots.some((root) => !root.readable)}
      <p class="warning">
        접근 불가 클라우드 루트는 선택에서 제외했습니다. macOS 개인정보 보호 권한을 허용한 뒤 목록을 다시 불러오세요.
      </p>
    {/if}
    {#if rootIssues.length > 0}
      <p class="warning">
        클라우드 위치 {rootIssues.length}곳을 확인하지 못했습니다. 접근 권한을 확인한 뒤 목록을 다시 불러오세요.
      </p>
    {/if}
    {#if selectedRootDetails()?.provider === "icloud"}
      <div class="oauth-panel">
        <strong>macOS iCloud 저장 공간</strong>
        <button onclick={verifyProviderCapacity} disabled={checkingCapacity}>
          {checkingCapacity ? "iCloud 저장 공간 확인 중…" : "iCloud 남은 공간 확인"}
        </button>
        {#if capacityForSelectedRoot()?.evidence_kind === "provider-native-status"}
          <p class="capacity-ok">
            Apple 계정 상태 확인 완료
            · 원격 잔여 {fmtBytes(capacityForSelectedRoot()?.remaining_bytes ?? 0)}
          </p>
        {:else if capacityForSelectedRoot()}
          <p class="warning">
            {capacityUnavailableLabel(capacityForSelectedRoot()?.unavailable_reason ?? null)}
          </p>
        {:else}
          <p class="muted">macOS에서 iCloud 계정 상태를 확인합니다. 파일을 변경하지 않습니다.</p>
        {/if}
        {#key selectedRoot}
          <IcloudLocalEviction cloudRoot={selectedRoot} />
        {/key}
      </div>
    {:else if selectedRootDetails()}
      <div class="oauth-panel">
        {#if connectionForSelectedRoot()}
          <strong>{providerApiWriteConnected() ? "클라우드 업로드 연결" : "읽기 전용 연결 정보 발견"}</strong>
          <button
            onclick={verifyProviderCapacity}
            disabled={checkingCapacity || disconnecting || connecting}
          >
            {checkingCapacity ? "클라우드 연결·저장 공간 확인 중…" : "재시작 후 연결·저장 공간 확인"}
          </button>
          <button onclick={disconnectProvider} disabled={disconnecting || connecting || checkingCapacity}>
            {disconnecting ? "연결 해제 중…" : "업로드 연결 해제"}
          </button>
          {#if capacityForSelectedRoot()?.evidence_kind === "provider-api"}
            <p class="capacity-ok">
              클라우드 계정과 저장 공간 확인 완료
              {#if capacityForSelectedRoot()?.remaining_bytes !== null}
                · 원격 잔여 {fmtBytes(capacityForSelectedRoot()?.remaining_bytes ?? 0)}
              {:else}
                · 저장 공간 제한 없음
              {/if}
            </p>
          {:else if capacityForSelectedRoot()}
            <p class="warning">
              {capacityUnavailableLabel(capacityForSelectedRoot()?.unavailable_reason ?? null)}
            </p>
          {:else}
            <p class="muted">
              연결 정보만 확인했습니다. 재시작 후 클라우드 연결과 저장 공간을 다시 확인하십시오.
            </p>
          {/if}
        {:else}
          <label>
            {selectedRootDetails()?.provider === "onedrive" ? "Microsoft 클라우드 연결 ID" : "Google 클라우드 연결 ID"}
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
            기본 연결이 불안정할 때 클라우드 업로드를 위한 추가 권한 요청
          </label>
          <button onclick={connectProvider} disabled={connecting || !oauthClientId.trim()}>
            {connecting ? "브라우저 동의 대기 중…" : "클라우드 업로드 연결"}
          </button>
          <p class="muted">
            연결 정보는 DiskSage가 안전하게 처리하며, 선택한 작업에 필요한 권한만 요청합니다.
          </p>
          {#if selectedRootDetails()?.provider === "onedrive"}
            <p class="muted">연결이 되지 않으면 클라우드 앱에서 새 연결 정보를 만든 뒤 다시 시도하십시오.</p>
          {/if}
          {#if selectedRootDetails()?.provider === "google-drive"}
            <p class="warning">Google 연결은 데스크톱 앱 유형이어야 합니다. 업로드 권한에 동의하지 않으면 파일을 읽고 확인하는 작업만 가능합니다.</p>
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
        클라우드 동기화가 끝나지 않았거나 상태를 확인할 수 없습니다. 새 복사를 잠시 막았으니,
        동기화가 끝난 뒤 계획을 다시 실행하십시오.
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
          디스크 여유 공간 확인 결과를 저장하지 못했습니다. 잠시 후 상태를 다시 확인한 뒤 실행하십시오.
        </p>
      {/if}
      {#if report.candidates.some(nativeCopyHeadroomBlocked)}
        <p class="warning">
          이 파일을 복사하려면 후보 크기와 {fmtBytes(api.LOCAL_COPY_RESERVE_BYTES)}의 여유 공간이 필요합니다.
          여유 공간을 확보한 뒤 다시 계획하십시오.
        </p>
      {/if}
    {/if}
    {#if hasRuntimeEvidencePersistenceFailure(report.notices)}
      <p class="warning">
        클라우드 앱 상태를 저장하지 못했습니다. 잠시 후 상태를 다시 확인한 뒤 계획을 다시 실행하십시오.
      </p>
    {/if}
    {#if hasIcloudHealthEvidencePersistenceFailure(report.notices)}
      <p class="warning">
        iCloud 동기화 상태를 저장하지 못했습니다. 상태가 정상화된 뒤 계획을 다시 실행하십시오.
      </p>
    {/if}
    {#if report.pre_copy_evidence && !report.pre_copy_evidence.complete}
      <p class="warning">
        복사 전 상태 확인이 끝나지 않아 새 복사를 막았습니다. 상태를 다시 확인한 뒤 계획을 다시 실행하십시오.
      </p>
    {/if}
    {#if report.capacity}
      {#if report.capacity.can_fit === true}
        <p class="capacity-ok">
          클라우드 저장 공간 확인됨 · 요청 {fmtBytes(report.capacity.requested_bytes)} + 보존 여유
          {fmtBytes(report.capacity.reserve_bytes)}
          {#if report.capacity.snapshot.remaining_bytes !== null}
            · 원격 잔여 {fmtBytes(report.capacity.snapshot.remaining_bytes)}
          {:else}
            · 저장 공간 제한 없음
          {/if}
        </p>
      {:else if report.capacity.can_fit === false}
        <p class="warning">
          클라우드 저장 공간이 부족합니다. 공간을 확보한 뒤 다시 계획하십시오.
        </p>
      {:else}
        <p class="warning">
          클라우드 저장 공간을 확인할 수 없습니다.
          {#if api.cloudNativeClientCopyAllowed(report.capacity, selectedRootDetails(), report.notices)}
            OneDrive 또는 Google Drive 앱이 실행 중인지 확인한 뒤 다시 계획하십시오. 업로드가 확인될 때까지 원본은 보존됩니다.
          {:else}
            OneDrive 또는 Google Drive 연결을 확인한 뒤 다시 계획하십시오.
          {/if}
          iCloud는 macOS 계정 상태를 확인한 뒤 다시 계획할 수 있습니다.
        </p>
      {/if}
    {/if}
    {#if report.exact_duplicates.candidate_count > 0}
      <p class="warning">
        정확 중복 {report.exact_duplicates.candidate_count.toLocaleString()}개 ·
        {report.exact_duplicates.cluster_count.toLocaleString()}개 콘텐츠 클러스터 ·
        대표본 외 중복 경로 {fmtBytes(report.exact_duplicates.redundant_bytes)}.
        같은 파일로 보이는 후보 {report.exact_duplicates.clusters.length.toLocaleString()}건은 자동으로 복사하지 않습니다.
        목록을 검토해 보관할 파일을 직접 선택하십시오. 낮은 확신의 추천은
        {report.exact_duplicates.clusters.filter((cluster) => cluster.recommendation_confidence === "low").length.toLocaleString()}건입니다.
      </p>
    {/if}
    <p class="warning">
      파일의 생산일과 경로를 확인해 보관 위치를 제안합니다. 이미 있는 클라우드 파일은 내용이 같은 경우에만 사용하며,
      원본은 업로드 확인과 별도 승인이 끝난 뒤에만 휴지통으로 이동합니다. 시스템 관리 데이터를 삭제하지 않습니다.
      휴지통은 자동으로 비우지 않습니다.
    </p>
    {#if copied}
      <div class="receipt">
        <strong>{copied.goal_status === "blocked" ? "복사 완료 · 추가 확인 필요" : copied.action === "adopt-existing-copy" ? "기존 클라우드 파일 확인 완료" : "클라우드 복사 완료"} · 원본 보존됨</strong>
        <div class="context">복사한 파일 {fmtBytes(copied.receipt.bytes)}</div>
        <div class="path">{copied.receipt.destination}</div>
        {#if copied.receipt.provider === "google-drive"}
          <div class="provider-auth">
            <label>
              업로드 확인용 파일 ID (선택)
              <input type="text" bind:value={objectId} autocomplete="off" disabled={attesting} />
            </label>
          </div>
          <p class="muted">클라우드 업로드 상태를 먼저 확인합니다. 파일 ID를 입력하면 업로드 확인 범위를 넓힐 수 있습니다.</p>
        {:else if copied.receipt.provider === "onedrive"}
          <p class="muted">클라우드 업로드 상태를 먼저 확인합니다. 연결이 필요하면 위의 클라우드 연결을 설정하십시오.</p>
        {/if}
        <button
          onclick={attestCopy}
          disabled={attesting}
        >
          {attesting ? "검증 중…" : "클라우드 업로드 상태·콘텐츠 확인"}
        </button>
        {#if attestation}
          <p class:warning={attestation.goal_state !== "eviction-ready"} class:safe={attestation.goal_state === "eviction-ready"}>
            클라우드 확인 상태 ·
            {syncStateLabel(attestation.evidence.sync_state)}
          </p>
          {#if attestation.assessment.state === "overdue"}
            <p class="warning">
              클라우드 업로드 확인이 {Math.floor(attestation.assessment.pending_age_ms / 3_600_000)}시간째 완료되지 않았습니다. 원본은 계속 보존하며 클라우드 상태를 다시 확인하십시오.
            </p>
          {:else if attestation.assessment.state === "pending"}
            <p class="muted">
              클라우드 업로드 확인 대기 {Math.floor(attestation.assessment.pending_age_ms / 60_000)}분. 완료 전에는 원본을 제거하지 않습니다.
            </p>
          {/if}
          {#if attestation.permit}
            {#if eviction}
              <p class="safe">원본을 운영체제 휴지통으로 이동했습니다. 클라우드 목적지는 유지되며 휴지통은 비우지 않았습니다.</p>
            {:else}
              <p class="safe">업로드 상태와 파일 내용을 확인했습니다. 원본은 아직 그대로 보존됩니다.</p>
              <div class="eviction-controls">
                <p class="warning">
                  아래 확인 문구를 직접 입력하고 이 파일만 휴지통으로 옮기는 사유를 남겨야 합니다. 실행 전에 파일 상태를 다시 확인하며 달라지면 중단합니다.
                </p>
                <div class="context">이 파일의 확인 문구: {copied.receipt.receipt_id}</div>
                <label>
                  전체 확인 문구
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
                    placeholder="예: 클라우드 업로드와 파일 확인을 마친 원본만 휴지통으로 이동"
                  ></textarea>
                </label>
                <button onclick={evictVerifiedSource} disabled={!sourceEvictionReady()}>
                  {evicting ? "상태를 다시 확인한 뒤 이동 중…" : "확인 후 원본을 휴지통으로 이동"}
                </button>
              </div>
            {/if}
          {:else}
            <p class="warning">아직 원본을 이동할 수 없습니다. 클라우드 상태를 다시 확인하십시오.</p>
          {/if}
        {/if}
      </div>
    {/if}
    {#if report.candidates.length === 0}
      <p class="muted">현재 크기·경과일·지원 파일 유형 조건에 맞는 후보가 없습니다. 조건을 조정한 뒤 다시 미리보기 하십시오.</p>
    {:else}
      <div class="review-queue" aria-label="클라우드 파일 검토 목록">
        <div class="review-progress" aria-live="polite">
          <strong>
            검토 진행 {reviewStats.reviewed.toLocaleString()} / {reviewStats.reviewable.toLocaleString()}개
          </strong>
          <progress
            max={Math.max(1, reviewStats.reviewable)}
            value={reviewStats.reviewed}
            aria-label={`파일 정보 검토 ${reviewStats.reviewed}개 완료, ${reviewStats.unreviewed}개 남음`}
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
                <option value={reason}>{customerDecisionReasonLabel(reason)}</option>
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
        <p class="muted">현재 상태·사유 필터에 맞는 후보가 없습니다. 필터를 바꾸거나 전체를 선택하십시오.</p>
      {:else}
      <ul class="candidates">
        {#each reviewPageData.items as candidate (candidate.metadata_fingerprint)}
          <li class:blocked={candidate.blocked_reason !== null} class:adoptable={adoptEligible(candidate)}>
            <div class="line">
              <strong>{fmtBytes(candidate.bytes)}</strong>
              <span>{candidate.kind}</span>
              <span>생산 {productionDate(candidate.production_time_ms)}</span>
              <span>{productionTimeConfidenceLabel(candidate.production_time_confidence)}</span>
              <span>수정 후 {candidate.age_days.toLocaleString()}일</span>
              {#if candidate.requires_review}<em>맥락/민감정보 검토 필요</em>{/if}
              {#if candidate.blocked_reason}<em>{customerDecisionReasonLabel(candidate.blocked_reason)}</em>{/if}
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
                  데이터 구조: {candidate.dataset_profile.format.toUpperCase()} ·
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
                  {candidate.dataset_profile.profile_complete ? "파일 구조 확인 완료" : "파일 구조를 모두 확인하지 못함·검토 필요"}
                  {candidate.dataset_profile.sample_truncated ? " · 제한 범위까지만 읽음" : ""}
                </div>
                {#if candidate.dataset_profile.columns.length > 0}
                  <ul class="schema-columns">
                    {#each candidate.dataset_profile.columns as column}
                      <li>
                        {column.name}: {column.inferred_type} · 확인 {column.observed_values.toLocaleString()} ·
                        결측 {column.missing_values.toLocaleString()}
                        {#if column.sensitive_name}<em>민감정보로 보이는 열 이름</em>{/if}
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
              맥락: {candidate.source_context} · 대상 계정: {accountScopeLabel(candidate.destination_account_scope)}
            </div>
            <details class="lineage">
              <summary>원본과 저장 위치</summary>
              <ol>
                <li>
                  원본 · {candidate.source_context}
                </li>
                <li>
                  생산 시점 · {productionDate(candidate.production_time_ms)}
                </li>
                <li>보관 위치 · {candidate.dst} · {fmtBytes(candidate.bytes)}</li>
              </ol>
              <p class="context">
                파일을 복사하고 업로드를 확인한 뒤 원본 이동을 별도로 승인합니다.
                {candidate.blocked_reason
                  ? ` 현재 진행할 수 없습니다: ${customerDecisionReasonLabel(candidate.blocked_reason)}.`
                  : " 아직 복사와 업로드 확인이 끝나지 않았습니다."}
              </p>
            </details>
            {#if candidate.requires_review}
              <div class="review-controls">
                {#if matchingReviewDecision(candidate)?.disposition === "approved"}
                  <strong class="approved">현재 파일 정보 검토 승인됨</strong>
                {:else if matchingReviewDecision(candidate)?.disposition === "held"}
                  <strong class="held">현재 파일 정보 보류됨</strong>
                {:else if reviewDecision(candidate)}
                  <strong class="held">파일 정보가 바뀌어 이전 결정이 만료됨</strong>
                {:else}
                  <span class="context">아래 정보를 확인한 뒤 승인 또는 보류하세요.</span>
                {/if}
                {#if matchingReviewDecision(candidate)}
                  <span class="context">
                    이전 검토 내용이 있습니다. 필요하면 새 사유를 입력하십시오.
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
                    이 조직이 해당 민감 자료를 보관할 권한이 있음을 확인했습니다.
                  </label>
                {/if}
                <button
                  onclick={() => reviewCandidate(candidate, "approved")}
                  disabled={reviewingFingerprint !== ""
                    || !(reviewRationales[candidate.metadata_fingerprint] ?? "").trim()
                    || (organizationTenantAuthorityRequired(candidate)
                      && !(reviewTenantAuthorities[candidate.metadata_fingerprint] ?? false))}
                >
                  {reviewingFingerprint === candidate.metadata_fingerprint ? "저장 중…" : "파일 정보 검토 승인"}
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
                <div class="context">현재 파일과 저장 위치를 확인한 뒤 승인 문구를 정확히 입력하십시오.</div>
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
                  {copyingFingerprint === candidate.metadata_fingerprint ? "복사 상태 확인 중…" : "원본을 유지하고 클라우드에 복사"}
                </button>
                {/if}
                {#if providerApiCopyEligible(candidate)}
                  <p class="warning">기본 클라우드 동기화가 지연되어 연결된 클라우드 서비스로 업로드합니다. 원본은 유지되며 업로드 확인 후 다음 단계로 진행합니다.</p>
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
                    {copyingFingerprint === candidate.metadata_fingerprint ? "클라우드 업로드 중…" : "연결된 클라우드 서비스로 업로드"}
                  </button>
                {/if}
              </div>
            {/if}
            {#if adoptEligible(candidate)}
              {@const adoptApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")}
              <div class="copy-approval">
                <div class="context">기존 클라우드 파일이 같은 내용인지 확인한 뒤 별도로 승인하십시오.</div>
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
                  {copyingFingerprint === candidate.metadata_fingerprint ? "기존 파일 확인 중…" : "기존 클라우드 파일 확인 후 사용"}
                </button>
              </div>
            {/if}
            <details>
              <summary>파일 정보 {candidate.metadata_evidence.length}건</summary>
              <ul class="evidence">
                {#each candidate.metadata_evidence as evidence}
                  <li>{evidence.field}: {evidence.value}</li>
                {/each}
              </ul>
            </details>
            {#if candidate.review_reasons.length > 0}
              <div class="context">검토 사유: {candidate.review_reasons.map(customerDecisionReasonLabel).join(", ")}</div>
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
