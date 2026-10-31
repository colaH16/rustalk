# Rustalk 🦀

GoTalk을 Rust로 옮긴 PostgreSQL + NATS + SSE 채팅 서버입니다. 원본 Go 코드를 옆에 두고 Rust의 소유권, 비동기 실행, 채널, 오류 처리와 운영 전환을 공부하기 위한 프로젝트입니다.

- 원본: [colaH16/gotalk](https://github.com/colaH16/gotalk/tree/1f1bc5a86f3c99c169010dfc496628da6f16c478)
- [Go → Rust 비교 학습](docs/GO_TO_RUST.md)
- [환경변수와 실행](docs/CONFIGURATION.md)
- [호환성·한계·운영 절차](docs/OPERATIONS.md)
- [검증 기록](docs/VALIDATION.md)

**2026-10-31 개인 Kubernetes 클러스터의 3개 Pod를 Rustalk으로 전환했습니다.** amd64/arm64 CI, Go 비교 테스트, 운영 읽기 전용 검증과 외부 브라우저 확인 결과는 [검증 기록](docs/VALIDATION.md)에 있습니다. [이력서용 설명과 면접 질문](docs/PORTFOLIO.md)도 함께 정리했습니다.

기존 `/login`, `/update`, `/history`, `/send`, `/stream` API와 `messages`/`users` 테이블, NATS `chat.global` 메시지 JSON을 유지합니다. Go/Rust 서버가 같은 DB와 NATS를 사용하면서 점진적으로 교체될 수 있습니다. UI의 기존 `cotalk_nick`·`cotalk_color` 저장값도 이어받습니다.

`/login`은 닉네임의 색상을 찾는 기능이며 사용자 인증이 아닙니다. 닉네임 소유권이나 채팅방 격리를 제공하지 않습니다. 기존 외부 접근 제어가 필요한 환경에서는 그 경계를 유지해야 합니다.

## 성능 비교: Go GC 켬 · GC 끔 · Rust

2026-10-31, 공유 Linux x86_64 VM에서 실제 측정했습니다. **JSON 직렬화 약 1.74배, 로그인 p50 약 1.24배, 단일 메시지 저장 p50 약 2.01배**의 개선을 관측했습니다. 아래는 반복 실행 중앙값이며 작은 ns 개선은 체감 성능과 구분해 읽어 주세요.

| 작업 / 지표 | Go GC 켬 | Go GC 끔 | Rust | Go GC 켬 대비 |
|---|---:|---:|---:|---:|
| 작은 메시지 JSON 직렬화 | 178.25 ns/op | 219.07 ns/op | 102.39 ns/op | 1.74배, 약 76 ns 절감 |
| form 파싱 | 346.65 ns/op | 456.59 ns/op | 244.56 ns/op | 1.42배, 약 102 ns 절감 |
| 로그인 조회 p50, 동시 1 | 434.47 μs | 395.54 μs | 350.06 μs | 1.24배 |
| 히스토리 조회 p50, 동시 1 | 782.10 μs | 993.91 μs | 518.18 μs | 1.51배 |
| 메시지 저장 p50, 동시 1 | 53.63 ms | 52.58 ms | 26.74 ms | 2.01배 |
| 히스토리 조회 처리량, 동시 8 | 346.66 req/s | 339.88 req/s | 4,621.95 req/s | 13.33배 |
| 같은 동시 8 실험의 최대 RSS | 15.64 MiB | 108.52 MiB | 7.16 MiB | 약 54.2% 감소 |

Rust의 GC 부재도 전환의 장점입니다. 동시 8 히스토리 실험에서 Go GC 켬은 trial당 GC 30~31회, 측정 중 STW 합계 8.43~13.95ms였고, Go GC 끔은 0회였습니다. Rust는 tracing GC 자체가 없습니다. GC를 끈 Go가 항상 빨라지지는 않았으며 메모리 사용량은 크게 늘었습니다.

**13.33배는 이 입력·동시성에서 기존 구현 전체를 비교한 처리량입니다.** DB 드라이버·연결 풀 정책도 다르고, 저장 경로는 Go의 두 autocommit 쓰기에서 Rust의 한 트랜잭션으로 바뀌었습니다. 따라서 GC 제거만으로 13배가 되었다는 뜻은 아닙니다. p99가 일관되게 개선된 것도 아닙니다. HTTP 3회·microbenchmark 7회, 모든 원시값·관측 범위·재현 코드·GC 해석은 [상세 성능 기록](docs/PERFORMANCE.md)에 공개합니다. 실제 운영 트래픽이나 클러스터 전체 처리량을 측정한 결과는 아닙니다.

## 실행

Rust 1.99.1, Node 24 이상, PostgreSQL 및 NATS를 준비합니다. 컨테이너를 쓸 수 있다면 테스트 의존성은 `docker compose up -d`로 시작할 수 있습니다.

```sh
npm ci --prefix frontend
npm run build --prefix frontend
export DB_HOST=127.0.0.1 DB_PORT=25432 DB_USER=rustalk_test
export DB_PASSWORD=disposable-test-only DB_NAME=postgres
export NATS_URL=nats://127.0.0.1:14222
# 새 로컬 DB에만 사용. 운영 전환에서는 false.
export DB_INIT_SCHEMA=true
cargo run --locked
```

브라우저에서 `http://localhost:8080`에 접속합니다. 모든 실행 설정은 환경변수입니다. 이미지의 ENTRYPOINT는 실행 파일 하나이며 Kubernetes `command`/`args`를 지정할 필요가 없습니다.

이미지는 CI에서 amd64/arm64 각각 테스트한 뒤 `ghcr.io/colah16/rustalk:sha-<전체 커밋>`으로 게시합니다. 운영에서는 해당 태그의 manifest digest를 고정합니다. Dockerfile은 CI에서 만든 `target/release/rustalk`와 정적 파일을 패키징합니다.

## 검증 재현

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
node --test tests/frontend.test.cjs
# 원본 Go repo를 NOTICE의 커밋으로 checkout한 뒤 backend에서:
# go build -o /tmp/gotalk-reference .
python3 -m venv .venv
.venv/bin/pip install -r tests/requirements.txt
TEST_ALLOW_RESET=1 GOTALK_BINARY=/tmp/gotalk-reference \
  .venv/bin/python -u tests/compare.py
```

비교 테스트는 **폐기 가능한 전용 DB**에서만 실행합니다. `rustalk_test_*` 데이터베이스를 삭제·재생성합니다. 정상 응답의 시간·Pod 이름은 형식을 검증하고 비교에서 제외하며, 같은 NATS로 전달되는 실시간 JSON은 원문 필드값 전체를 비교합니다.

Apache-2.0. GoTalk에서 가져온 프런트엔드와 스키마의 출처 및 변경 사항은 [NOTICE](NOTICE)에 기록했습니다.
