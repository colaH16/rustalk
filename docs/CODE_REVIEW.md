# Claude 독립 코드 검토와 조치 (2026-10-31)

사용자 요청으로 Claude Code (`claude-opus-5-5`)에 공개 소스만 읽기 전용으로 전달했습니다. Go 원본과 Rust HTTP/DB/NATS/SSE/설정/프런트엔드를 비교했고, 후속 검토에서는 벤치마크를 제외하고 수정된 코드를 다시 읽었습니다. Claude는 테스트를 실행하지 않았으며 아래 실행 검증은 별도로 수행했습니다. AI 검토는 전체 정확성이나 보안의 보증이 아닙니다.

반영한 항목:

- 공유 NATS/DB 장애가 모든 Service endpoint를 제거하지 않도록 readiness와 `/dependencies` 진단 분리.
- NATS 단절 시 DB에 저장 후 성공 응답. 연결되지 않았으면 live publish 대기를 생략. 복구는 history catch-up, Core NATS 전달 보장은 여전히 best effort.
- `DB_TIMEZONE` 명시, INSERT가 반환한 DB 시각을 live 이벤트에도 사용. 기존 DB의 `SHOW timezone`과 동일하게 설정해야 함.
- 구독이 비정상 종료되면 nonzero 종료, SQLSTATE/오류 분류만 기록.
- `#RRGGBB` 외 색상 입력은 기본색으로 정규화하고 프런트엔드에서도 속성 객체와 검증 사용. 과거 데이터는 수정하지 않음.
- catch-up 중 재연결 요청을 보존하고 실패 후 jitter 재시도. 대량 메시지의 spread 인자 제한 제거.
- SIGTERM 후 ready=false와 SSE 종료를 먼저 알리고 기본 5초 listener 유지. 전체 drain timeout 이내에 종료.

추가 검증: 기존 56 Go/Rust 응답 쌍, 혼합 SSE/DB rollback/NATS 단절·재연결/시간대/정상 종료 통합 검사, 프런트엔드 5개 회귀 테스트. 유효하지 않은 색상을 정규화하는 동작은 Go 원본 대비 의도적 보강입니다.

Claude 제안을 그대로 수용하지 않은 사례도 있습니다. `before_id` 정수 overflow가 Go에서 최신 기록을 반환한다는 가정은 실제 비교에서 DB 오류가 나서 확인되지 않았으므로 그 가정에 맞춘 변경을 제거했습니다. axum 0.8.9 Event.data가 CR 문자로 panic한다는 가설은 사용 중인 라이브러리 소스가 CR/LF를 분리 처리하므로 확인되지 않았습니다. Go와 Rust의 잘못된 입력에 대한 모든 오류 응답 호환성은 보장하지 않습니다.

성능 결과는 이 수정 전 pinned commit의 측정이며 그대로 보존합니다. 전용 부하 환경, 장기 soak, 인증/닉네임 소유권, outbox/idempotency는 별도 범위입니다.
