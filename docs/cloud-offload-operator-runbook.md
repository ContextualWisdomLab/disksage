# DiskSage cloud offload operator runbook

이 문서는 `/Users/seonghobae/Downloads` 같은 로컬 원본을 iCloud Drive, OneDrive,
Google Drive에 보관할 때의 운영 순서를 정의한다. 계획·복사·원본 회수는 서로 다른
상태이며, 앞 단계의 성공만으로 다음 단계를 승인하지 않는다.

## 1. 권한과 증거의 범위

- 로컬 스캔과 내장 메타데이터 판독은 클라우드 OAuth 없이 수행한다.
- File Provider 루트 탐지에는 macOS 개인정보 보호 권한이 필요할 수 있다.
- OneDrive와 Google Drive의 원격 용량·계정 소유권 확인에는 OAuth PKCE 연결이 필요하다.
  Desktop public client ID만 사용하며 client secret은 저장하거나 입력하지 않는다.
- iCloud는 macOS 네이티브 quota 상태를 사용하지만, quota만으로 업로드 완료를 증명하지
  않는다.

## 2. 후보 판정

생산일 증거 우선순위는 다음과 같다.

1. 파일 내부 메타데이터(EXIF, ffprobe, 문서 core properties, ZIP central directory 등)
2. 명시적인 파일명 날짜(저신뢰 보조 힌트)
3. 파일시스템 생성 시각
4. 파일시스템 수정 시각

`2026-04-28`이나 `251210` 같은 파일명 토큰만으로 생산일을 확정하지 않는다. 내장
메타데이터와 파일명 날짜가 충돌하면 후보를 검토 상태로 둔다. `.crdownload`, 누락된
multipart archive, 읽을 수 없는 archive index는 원자적 복사 계획이 없으면 차단한다.

메타데이터 도구와 중복 content hash도 계획 전체 예산 안에서만 실행한다. 초기 계획은
가장 큰 eligible 파일 최대 32개와 10초의 외부 probe 예산, 16 MiB의 중복 hash 예산을
사용한다. 예산을 넘긴 후보에는 `metadata-probe-status` 또는 content-hash 지연 증거와
`content-metadata-probe-deferred`/`exact-duplicate-content-probe-deferred` 검토 사유가
남고, 보고서에는 해당 지연 notice가 추가된다. 이 후보를 복사하려면 새 계획에서 필요한
메타데이터와 digest를 다시 확인해야 한다.

캐시 메타데이터 manifest도 항목당 2초 또는 100,000개 record에서 멈춘다. 이 경우
`scan_complete=false`와 `metadata-manifest-bounded`가 남으며, 읽힌 bytes는 부분값일 수
있다. 불완전 manifest는 GUI와 Rust 정리 게이트에서 자동 거부되므로, 새 읽기 전용 계획이
완료된 뒤에만 별도 항목 승인으로 진행한다.

## 3. 계획과 복사

1. DiskSage에서 원본 루트를 스캔하고 클라우드 루트를 다시 탐지한다.
2. 후보의 `metadata_fingerprint`, `review_fingerprint`, bytes, 원본 상대 경로,
   production-time source/confidence, context를 검토한다.
3. 공급자 용량과 동기화 상태를 검증한 뒤 계획을 다시 생성한다. 이전 preview나
   로컬 provider 폴더 존재만으로는 복사 승인을 재사용하지 않는다.
4. 민감 맥락·저신뢰 생산일·컨테이너 내용을 가진 후보는 해당 fingerprint에 결박된
   명시적 approve/hold 결정이 있어야 한다.
5. 복사는 `create-only`와 콘텐츠 hash 검증을 거치며, 원본은 그대로 둔다. copy-only
   receipt의 `lineage.capacity`에는 그 복사를 허용한 용량 snapshot, evidence fingerprint,
   requested/reserve 계산, `can_fit` 결과가 함께 결박된다. immutable receipt와 provider
   evidence가 생성되어야 복사 단계가 완료된 것으로 본다. 이미 존재하는 동일 목적지를
   채택하는 경로는 새 바이트를 쓰지 않으므로 capacity lineage가 없을 수 있다.

## 4. 원본 회수

원본 회수는 복사와 별도의 승인이다. provider-native/API evidence가 receipt의
destination, bytes, digest, 위치와 일치하고 `sync_complete`인 경우에만 eviction
permit이 생성된다. permit 없이 원본을 Trash로 보내지 않는다. 회수 전에는 source
metadata와 content digest를 다시 확인하고, 실패하면 staging을 복구한다.

## 5. 승인 문구의 범위

`승인`, `네` 같은 일반 동의는 현재 후보에 결박되지 않는다. 실행 직전에 DiskSage가
새 계획을 만들고 다음 항목을 제시해야 한다.

- 정확한 source/destination 경로
- bytes와 source modified 시각
- metadata/review fingerprint
- 공급자·계정 범위·용량 evidence fingerprint
- copy-only인지, provider attestation인지, source eviction인지

사용자는 copy-only와 source eviction을 각각 승인한다. 어느 한 단계의 성공을 다음
단계의 승인으로 간주하지 않는다.

## 6. stale Git worktree 감사

`disksage-git-worktree-audit`는 `git worktree list --porcelain`을 5초 안에 끝내지
못하면 `.git/worktrees` 관리자 등록을 읽기 전용으로 확인한다. 관리자 파일은 크기와
읽기 시간을 제한하며, 비어 있거나 읽기 timeout인 `gitdir`는 실제 worktree 경로로
추정하지 않고 `<worktree-admin:...>` 증거로 남긴다. 이 fallback 보고서는
`evidence_complete: false`이므로 `registration_fingerprint`를 보관하고 수동 검토할
때까지 `git worktree prune/remove`나 파일 삭제를 실행하지 않는다. 완전한 감사에서
`metadata_prune_eligible_count`가 양수이면 UI의 명시적 승인 문구를 통해
`prune_stale_worktree_metadata`를 실행할 수 있다. 이 명령은 재감사와 fingerprint
일치를 먼저 확인하고 `git worktree prune --expire now`만 실행한다. worktree 디렉터리,
브랜치, 파일은 삭제하지 않으며 사후 감사에서 stale 등록 감소를 확인하지 못하면 실패한다.

## 7. 조건부 통합 경계

기본 판단·hash·capacity 계산은 Rust와 오프라인 llama.cpp 경로를 사용한다(Ollama
사용 안 함). Noema/contextual-orchestrator는 실제 agent/external-LLM 계약이 생길
때만 연결한다. semantic-data-portal과 pg-erd-cloud는 영속 catalog/DB 경계가 필요할
때만 연결하고, fast-mlsirm은 binary/polytomous LLM-as-a-Judge 계약이 생길 때만
판정기로 사용한다.
