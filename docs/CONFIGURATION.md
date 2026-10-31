# 환경변수

CLI 인자는 받지 않습니다. 이미지 ENTRYPOINT는 `/usr/local/bin/rustalk` 하나입니다. Deployment에서는 필요한 `env` 항목만 추가하거나 변경합니다. 비밀값은 Secret의 `valueFrom`을 사용합니다.

| 변수 | 기본값 | 의미 |
|---|---|---|
| `LISTEN_ADDR` | `0.0.0.0:8080` | HTTP bind 주소 |
| `STATIC_DIR` | 바이너리 `frontend`, 이미지 `/app/static` | index.html/CSS/Alpine 경로 |
| `DB_HOST` | `127.0.0.1` | PostgreSQL 호스트 |
| `DB_PORT` | `5432` | PostgreSQL 포트 |
| `DB_USER` | `postgres` | DB 사용자 |
| `DB_PASSWORD` | 빈 값 | DB 암호; 로그 출력하지 않음 |
| `DB_NAME` | `cotalk` | **기존 이름을 그대로 설정** |
| `DB_SSLMODE` | `disable` | 기존 Go와 동일; TLS 환경은 `require`/`verify-full` 등 설정 |
| `DB_MAX_CONNECTIONS` | `10` | Pod당 최대 풀 크기 (1–1000) |
| `DB_TIMEZONE` | `UTC` | PostgreSQL 세션 시간대; 기존 timestamp 데이터의 기준에 맞춤. 실시간과 history는 같은 DB 시각 사용 |
| `DB_INIT_SCHEMA` | `false` | 새 DB에 기존 Go 테이블 생성 허용; DB 자체는 미리 생성 |
| `NATS_URL` | `nats://127.0.0.1:4222` | NATS 연결 URL |
| `NATS_SUBJECT` | `chat.global` | Go와 섞어 실행할 때 동일하게 유지 |
| `POD_NAME` | `HOSTNAME`, 없으면 `rustalk` | 메시지의 sender_pod |
| `SSE_BUFFER_CAPACITY` | `100` | Pod 내 공유 broadcast 링 버퍼, 1–1,000,000 |
| `SSE_KEEPALIVE_SECONDS` | `15` | SSE keepalive 간격 |
| `DEPENDENCY_TIMEOUT_SECONDS` | `5` | DB pool 획득/SQL statement, NATS 연결·flush/dependencies 검사 제한 |
| `DRAIN_DELAY_SECONDS` | `5` | 종료 시 ready=false 이후 listener 유지 시간; shutdown timeout보다 작게 |
| `SHUTDOWN_TIMEOUT_SECONDS` | `25` | HTTP 연결 drain 제한; terminationGracePeriod보다 작게 |
| `MAX_BODY_BYTES` | `1048576` | HTTP body 최대 크기, 최대 16 MiB; NATS max_payload도 고려 |
| `TZ` | 이미지 `UTC` | 프로세스 로컬 시간대. 메시지 시각은 `DB_TIMEZONE`을 사용 |
| `RUST_LOG` | `rustalk=info,warn` | tracing filter |

빈 환경변수는 기본값을 사용합니다. 잘못된 숫자·주소·bool은 기동 실패로 처리합니다. bool은 `true`/`false`입니다. 서버가 정상 기동하려면 정적 index.html, 기존 DB 스키마, DB와 NATS 연결이 필요합니다.

`/livez`는 HTTP 프로세스 생존, `/readyz`는 초기화 완료 후 종료 상태를 검사합니다. `/dependencies`는 NATS 연결·구독과 DB `SELECT 1`을 검사하는 별도 진단 경로입니다. 공유 의존성 장애로 모든 Service endpoint를 제거하지 않으므로 정적 페이지와 가능한 API를 계속 제공합니다. NATS 단절 중 send도 DB commit 후 200을 반환하며 실시간 전달은 best effort입니다. 이 진단 경로를 readiness/liveness로 사용하지 않습니다.

```yaml
# 예시: 애플리케이션 컨테이너의 일부. 서비스 주소는 예시입니다.
image: ghcr.io/colah16/rustalk@sha256:<tested-manifest-digest>
# command와 args는 생략
ports:
  - containerPort: 8080
env:
  - {name: TZ, value: Asia/Seoul}
  - {name: DB_TIMEZONE, value: Asia/Seoul}
  - {name: DB_HOST, value: postgres}
  - {name: DB_NAME, value: existing_chat}
  - {name: NATS_URL, value: 'nats://nats:4222'}
  - {name: DB_INIT_SCHEMA, value: 'false'}
  - name: DB_USER
    valueFrom:
      secretKeyRef: {name: chat-db, key: user}
  - name: DB_PASSWORD
    valueFrom:
      secretKeyRef: {name: chat-db, key: password}
readinessProbe:
  httpGet: {path: /readyz, port: 8080}
  periodSeconds: 5
  timeoutSeconds: 6
livenessProbe:
  httpGet: {path: /livez, port: 8080}
  periodSeconds: 10
  timeoutSeconds: 2
startupProbe:
  httpGet: {path: /livez, port: 8080}
  periodSeconds: 2
  failureThreshold: 30
```
