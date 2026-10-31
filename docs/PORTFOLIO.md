# 이력서와 설명 연습

## 짧은 프로젝트 설명

> Go 기반 실시간 채팅 서비스를 Rust(axum/Tokio)로 포팅하고, 기존 PostgreSQL·NATS·SSE 계약을 유지하며 개인 Kubernetes 클러스터의 3개 Pod를 실제 운영 전환했다. Go/Rust 응답 56쌍 비교와 혼합 버전·장애 주입 테스트를 amd64/arm64 CI에 구성하고, 환경변수 기반 실행 설정과 readiness·graceful shutdown을 적용했다.

운영 경험을 추가하려면:

> GitOps 전환 중 발견한 영속 볼륨의 Helm upgrade hook 문제를 수정했다. DB 백업 검증과 동일 PV 재바인딩 후 메시지·프로필 전체 지문을 대조하여 데이터 보존을 확인했다.

수치는 [검증 기록](VALIDATION.md)과 CI에서 확인할 수 있습니다. 성능 개선율, 무중단·무손실 또는 장기 운영 기간은 현재 증거로 주장하지 않습니다.

## 직접 설명해 볼 질문

1. Rust로 바꿨는데 왜 DB 스키마와 NATS JSON을 그대로 유지했나요?
2. `Arc`, `PgPool::clone`, broadcast receiver가 각각 무엇을 공유하고 소유하나요?
3. NATS queue subscription을 쓰면 왜 여러 Pod의 SSE 사용자에게 메시지가 빠지나요?
4. DB commit 뒤 NATS publish 실패를 DB transaction 하나로 해결할 수 있나요?
5. liveness와 readiness에 같은 검사를 넣으면 장애 때 어떤 문제가 생기나요?
6. 열린 SSE가 많은 상태에서 SIGTERM이 오면 어떤 순서로 종료하나요?
7. 이미지 기본 args가 Deployment args 전체 override 때문에 사라지는 문제를 어떻게 피했나요?
8. 왜 PVC protection finalizer를 강제로 제거하지 않았고, Retain과 백업은 각각 무엇을 보호하나요?
9. Go로 롤백할 때 이미지 외에 probe도 함께 되돌려야 하는 이유는 무엇인가요?
10. 테스트 assertion 288개와 독립 시나리오 288개는 어떻게 다른가요?

[Go → Rust 비교 학습](GO_TO_RUST.md)을 읽은 뒤 코드 위치와 실제 테스트를 함께 가리키면서 답해 보면 좋습니다.
