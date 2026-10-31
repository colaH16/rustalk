# 검증 기록

2026-10-31 개인 Kubernetes 클러스터의 기존 GoTalk을 Rustalk으로 전환했습니다. 서비스 주소, 사용자 데이터, 인증 정보와 DB 백업은 이 저장소에 포함하지 않습니다.

## 소스와 이미지

- Go 기준: `1f1bc5a86f3c99c169010dfc496628da6f16c478`.
- 배포한 Rust 소스: [`43356d31b312242a1fa28b74d71019197dafb8ca`](https://github.com/colaH16/rustalk/commit/43356d31b312242a1fa28b74d71019197dafb8ca). 이후 문서 커밋은 이미지 변경이 아닙니다.
- [GitHub Actions 41163890605](https://github.com/colaH16/rustalk/actions/runs/41163890605): amd64·arm64 각각 검사, 테스트, release binary 빌드 및 이미지 게시 성공. multiarch manifest job 성공.
- 공개 익명 pull 확인: `ghcr.io/colah16/rustalk@sha256:1099947057455dccc8cb0d940af847566ec1452421bfd1ca1a897a422cacfe2b`.

## 테스트 결과

- 로컬 및 각 아키텍처 CI: Go/Rust 응답 **56쌍 일치**, **288개 assertion 통과**. assertion 수에는 응답별 시각 형식 검사가 포함되므로 서로 독립적인 시나리오 288개라는 의미는 아닙니다.
- 혼합 Go/Rust 3개 서버: 양방향 SSE 전달, 동일 NATS JSON 수신, 중복 로컬 echo 없음.
- PostgreSQL lock timeout 및 프로필/메시지 트랜잭션 rollback.
- NATS 단절: readiness 503, liveness 200, DB 쓰기 전 거절, SSE 종료. 복구 후 readiness 및 구독 회복.
- 열린 SSE를 유지한 상태에서 SIGTERM 정상 종료.
- Rust unit test 2개, 프런트엔드 reconnect pagination·중복 제거·실패 후 상태 보존 테스트 2개.
- `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` 통과.
- 별도 Chromium 확인: 테스트 환경 두 사용자 송수신, 오프라인 후 기록 복구, 새로고침 후 닉네임 유지, CSS 렌더링, page JavaScript error 없음.

## 운영 전환 확인

- Service selector와 분리된 amd64/arm64 canary에서 기존 Go와 최근 기록 **30개가 동일**함을 확인.
- canary 및 최종 운영에서 health, 정적 HTML/CSS/Alpine, 읽기 전용 SSE 연결 확인. 운영 채팅으로 테스트 메시지를 보내지 않음.
- 컨테이너 UID `65532`, PID 1은 Rust 바이너리. Deployment `command`/`args` override 없음.
- 운영 Rustalk **3/3 Ready**, **재시작 0**, Service의 Ready endpoint **3개**, Fleet **1/1 Ready** 확인. amd64/arm64 노드 모두에서 실제 실행.
- 기존 외부 경로를 일반 Chromium으로 열어 HTTP 200 및 Rustalk 제목 확인. Python HTTP client는 전환 전에도 Cloudflare browser integrity 정책에 의해 거절되었으므로 브라우저로 검증함.
- 임시 canary와 폐기용 DB/NATS namespace 정리. Fleet pause 해제.

## 전환 중 발견한 기존 DB hook 문제

첫 GitOps 적용에서 기존 DB PV/PVC/Deployment/Service가 `pre-install,pre-upgrade` hook으로 지정된 문제가 드러났습니다. Helm의 기본 before-hook-creation 정책이 PVC 삭제를 요청했고, 사용 중인 PVC의 protection finalizer가 삭제를 보류했습니다. 앱 rollout은 이 지점에서 중단됐습니다.

Fleet를 pause하고 비공개 DB dump를 확보·해독 검증했습니다. 원본 GitOps에서 네 DB 리소스의 hook을 제거하고 정상 Helm 소유권을 명시했으며, PV/PVC에는 keep 정책을 추가했습니다. 실제 PV는 기존부터 Retain이었습니다.

이미 삭제 대기인 PVC는 취소할 수 없어 **DB를 잠시 정지**하고 protection finalizer가 자연스럽게 완료된 다음, 같은 이름의 PVC를 **같은 PV·같은 Ceph 저장 경로**에 재바인딩했습니다. DB를 재기동하고 메시지·프로필 전체 데이터 지문이 전후 일치함을 확인했습니다. 강제 finalizer 제거, 데이터 초기화, dump 복원은 하지 않았습니다. 이후 hook 없는 새 bundle을 확인하고 Fleet를 재개했습니다.

관련 원본 GitOps 변경:

- [`429aae8`](https://github.com/colaH16/gotalk-gitops/commit/429aae8e9ead59f9976c6dff93ae55d641e57438): Rust 이미지·env·probe.
- [`2f09bac`](https://github.com/colaH16/gotalk-gitops/commit/2f09bac): PV/PVC lifecycle 수정.
- [`fd881b6`](https://github.com/colaH16/gotalk-gitops/commit/fd881b65b246354fd5b66898153f0e929876075f): DB workload/Service lifecycle 수정.

이 전환 전체를 무중단이라고 주장하지 않습니다. 단기 운영 검증이며 성능 향상, 장기 안정성 또는 무손실 전달을 측정한 결과도 아닙니다.
