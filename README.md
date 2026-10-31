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
