# 운영과 호환성 범위

## 유지하는 계약

- 기존 URL과 JSON 필드, PostgreSQL 두 테이블, NATS subject와 일반 subscription.
- history는 최근 30개 ID 내림차순, before_id 미포함 경계, 빈 결과 `null`.
- 메시지 기록의 색상은 현재 users 색상을 JOIN. 실시간 JSON에는 발송 당시 색상 포함.
- `send`/`update`의 POST 이외 메서드와 필수값이 빈 요청은 기존처럼 empty 200.
- URLencoded body가 query보다 우선하고 중복 key는 첫 값 사용.
- 기존 브라우저 storage key, 외부 서비스 경로와 접근 제어 경계.

## 의도적으로 달라진 점

- DB 자동 생성은 제거. 테이블 생성은 `DB_INIT_SCHEMA=true`일 때만 허용하며 운영 기본값은 false.
- DB 오류를 unknown user로 숨기지 않고 503. SQL/암호/메시지 내용은 오류 응답에 포함하지 않음.
- DB 프로필/메시지 변경은 같은 트랜잭션, 연결 풀과 SQL 실행 시간 제한.
- NATS 단절을 알고 있을 때 DB 쓰기 전 503. commit 후 live 전달 실패는 저장 성공으로 응답하고 로그로 구별.
- request body 크기 제한; multipart는 415 (기존 웹 UI는 URLencoded만 사용).
- `/stream`은 GET/HEAD 라우팅. 느린 SSE 소비자와 NATS 연결 변경은 재연결 유도.
- 프런트엔드에서 HTTP 실패 상태 확인, 재연결 시 history pagination으로 빠진 메시지 보충, ID 중복 제거.
- Alpine을 npm lockfile에서 빌드에 포함하여 외부 CDN 런타임 의존 제거.
- non-root 이미지, readiness/liveness, SIGTERM drain. 모든 설정은 env.

NATS Core의 live delivery는 best effort입니다. SSE 자체에 내구성 있는 replay cursor가 없으며 DB 기록 보충도 읽기와 동시 commit의 모든 순서를 보장하지 않습니다. `send` 응답 중 네트워크가 끊겼을 때 서버의 commit 여부를 클라이언트가 확정할 수 없는 문제도 남아 있습니다. idempotency/outbox, 사용자 인증, rate limiting, 부하/장기 soak 검증은 별도 범위입니다. 단기 호환성 검증을 성능 개선의 근거로 사용하지 않습니다.

## 전환 절차

1. 원본 GitOps 저장소에서 `git status`, `git pull --ff-only`. 다른 변경은 보존. 앱뿐 아니라 DB/PV/PVC 등 같은 bundle의 hook·삭제 정책도 확인. 영속 리소스를 upgrade hook으로 만들지 않음.
2. Go 참조 커밋과 Rust 소스를 고정하고 폐기 가능한 DB/NATS에서 비교·혼합·장애 테스트.
3. CI가 amd64/arm64 각각 통과한 image manifest digest 확인.
4. 서비스 selector와 분리된 Rust canary로 기존 DB 스키마·readiness 및 **읽기 전용** history 비교. 실제 채팅으로 테스트 메시지를 보내지 않음.
5. 원본 Deployment의 이미지를 digest로 교체하고 env/probe를 명시. Service/Ingress/DB/NATS/Secret/PVC와 resource 이름은 유지.
6. GitOps commit/push 후 Fleet가 적용하는 rollout을 확인. 새 Pod readiness, 기존 Pod 종료, 서비스 endpoint, 정적 파일·history·SSE 확인.
7. 임시 canary와 테스트 namespace 삭제. 실제 결과만 검증 기록에 추가.

Go와 Rust는 같은 테이블·메시지를 사용하므로 점진적 전환 중 함께 실행할 수 있습니다. schema migration이 없으므로 롤백 시에도 데이터를 복사하거나 되돌리지 않습니다.

## 롤백

애플리케이션 변경 커밋 `429aae8`만 원본 GitOps에서 `git revert`하여 이전 Go 이미지와 probe 설정을 함께 복원하고 push합니다. Go에는 `/livez`/`/readyz`가 없으므로 **이미지만 되돌리면서 Rust probe를 남기면 안 됩니다**. Fleet의 drift correction을 사용하므로 수동 `kubectl set image`만 바꾸면 원복될 수 있습니다. 신규 대화 데이터는 그대로 유지합니다. DB hook을 제거한 후속 커밋은 유지해야 합니다. hook을 되살리면 다음 upgrade에서 PVC 삭제 요청이 재발합니다.

기존 Go CI는 Go 이미지 경로만 치환합니다. Rust 이미지로 전환된 Deployment에는 그 치환식이 매치되지 않지만, 이는 Rust 자동 배포 파이프라인이 구성되었다는 뜻은 아닙니다. Rust CI는 검사·이미지 게시까지만 담당하며 GitOps의 digest 승격은 명시적인 커밋으로 수행합니다.

## 영속 리소스와 hook

이번 전환에서 발견·복구한 사례는 [검증 기록](VALIDATION.md)에 있습니다. Helm hook은 일반 workload의 시작 순서 표시용 annotation이 아닙니다. 삭제 정책을 생략하면 다음 hook 실행 전에 기존 리소스를 삭제할 수 있습니다. DB/PV/PVC/Service는 일반 관리 리소스로 두고, 볼륨 보존 정책을 따로 확인해야 합니다. [Helm 공식 hook 문서](https://docs.helm.sh/docs/topics/charts_hooks/) 참조.
