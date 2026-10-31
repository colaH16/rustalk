# Rustalk 성능 실험

측정일: 2026-10-31. 공유 Linux x86_64 VM(노출 vCPU 10개), Go 1.25.5 / Rust 1.99.1 release. 전용 벤치마크 머신이 아니며 CPU affinity는 다른 작업을 배제하지 않습니다. Go 기본 GC(`GOGC=100`), 동일 Go 바이너리의 GC 끔(`GOGC=off`, `GOMEMLIMIT=off`), Rust를 순서를 회전하며 비교했습니다. GC off는 한정된 실험이며 운영 권장 설정이 아닙니다.

Rust는 tracing GC를 사용하지 않아 해당 GC cycle/STW가 없습니다. 메모리 해제·할당 비용과 OS 스케줄링 지연까지 없어지는 것은 아닙니다. Go GC는 동시 실행되는 부분과 짧은 STW 구간이 있습니다. GC trace의 전체 mark 시간은 STW가 아니므로 정지 시간에 합산하지 않았습니다. [Go GC 가이드](https://go.dev/doc/gc-guide), [측정 버전 runtime 문서](https://github.com/golang/go/blob/go1.25.5/src/runtime/extern.go).

microbenchmark는 CPU 2 하나에 고정하고 Go는 `GOMAXPROCS=1`로 실행했습니다. 동시 GC worker도 같은 CPU 시간을 나눠 씁니다. 프로세스마다 warmup 후 고정 반복 횟수를 측정하고 7회 중앙값을 냈습니다. 입력 준비·JSON 결과 출력·Go의 사전 `runtime.GC()`는 시간 밖입니다. Rust는 결과의 drop을 루프 안에서 수행하며 Go는 측정 중 GC 비용을 포함합니다. 따라서 GC off는 측정 이후의 메모리 회수 비용을 포함하지 않습니다. 최적화에 의한 제거는 Go 전역 sink와 Rust `black_box`로 막았습니다. 출력 샘플의 의미가 같은지도 실행기가 확인합니다. Go allocation 수는 `runtime.MemStats` 차이이고 Rust allocation 수는 측정하지 않았습니다. 첫 회를 포함한 모든 최종 측정값을 공개하며 오차 범위는 신뢰구간이 아닌 관측 최솟값~최댓값입니다.

## HTTP 비교

각 trial은 새 서버 프로세스·초기화한 전용 DB로 시작하고 100회 warmup 후 측정합니다. 서버는 CPU 2,3 / 클라이언트는 4,5에 고정, GOMAXPROCS=2 / TOKIO_WORKER_THREADS=2. PostgreSQL 18.0과 NATS 2.12는 서버와 같은 Kubernetes 노드에서 실행했고 Pod IP로 직접 연결했습니다. DB는 1 CPU/512Mi 제한, ephemeral filesystem과 기본 fsync입니다. 운영 DB·서비스에는 요청하지 않았습니다. port-forward로 진행한 초기 예비 실험은 네트워크 지연이 커 최종 실험 전에 제외했습니다. microbenchmark도 빌드가 끝난 후 전량 새로 측정했습니다.

히스토리는 128 ASCII byte 메시지 100개를 seed하고 API 기본 30개 응답을 읽습니다. login/history c1은 2,000회, history c8은 4,000회, send c1은 600회, send c4는 1,200회이며 각 조건 3회입니다. send는 동일 synthetic 닉네임으로 보내므로 c4에는 사용자 행 경합도 포함됩니다. keep-alive를 사용하는 closed-loop 클라이언트이며 성공 응답과 본문 수신 완료까지 측정했습니다. 모든 최종 trial의 HTTP 오류는 0입니다. 처리량은 이 부하에서 달성한 수치이며 최대 수용량 테스트가 아닙니다.

### p50 (μs; 중앙값과 관측 범위)

| 작업 | Go GC 켬 | Go GC 끔 | Rust |
|---|---:|---:|---:|
| login_c1 | 434.472 (431.895~439.116) | 395.536 (393.675~491.438) | 350.061 (286.719~361.639) |
| history_c1 | 782.102 (701.413~1,004.167) | 993.911 (828.136~1,012.575) | 518.176 (508.431~797.879) |
| history_c8 | 3,715.260 (3,627.183~3,725.255) | 3,845.124 (3,690.247~3,877.600) | 818.484 (761.362~865.057) |
| send_c1 | 53,629.703 (51,305.730~58,223.072) | 52,582.584 (48,581.152~57,077.985) | 26,735.914 (22,779.771~27,566.504) |
| send_c4 | 146,302.663 (141,004.598~167,585.063) | 143,317.014 (142,261.977~144,914.948) | 80,101.598 (77,517.225~83,384.756) |

### p99 (μs)

| 작업 | Go GC 켬 | Go GC 끔 | Rust |
|---|---:|---:|---:|
| login_c1 | 2,149.427 (1,257.883~2,330.480) | 1,731.890 (1,596.753~1,833.987) | 1,163.766 (1,070.378~2,247.135) |
| history_c1 | 2,576.730 (2,302.930~3,540.744) | 2,874.368 (2,359.783~3,568.857) | 1,684.950 (1,636.326~2,727.197) |
| history_c8 | 99,987.278 (99,542.839~100,483.808) | 100,612.357 (99,838.022~101,127.100) | 49,953.033 (22,887.797~51,606.992) |
| send_c1 | 111,351.880 (85,893.573~122,060.505) | 107,419.386 (91,829.356~144,993.147) | 52,012.744 (40,344.804~110,152.218) |
| send_c4 | 403,892.092 (366,083.226~753,499.727) | 368,091.864 (361,103.438~394,380.527) | 395,764.949 (378,651.806~418,631.326) |

### 처리량 (req/s)

| 작업 | Go GC 켬 | Go GC 끔 | Rust |
|---|---:|---:|---:|
| login_c1 | 1,932.076 (1,920.752~2,031.680) | 2,170.485 (1,771.969~2,183.014) | 2,366.767 (2,201.015~2,961.747) |
| history_c1 | 1,153.716 (920.689~1,217.197) | 913.848 (898.840~1,052.946) | 1,676.856 (1,119.569~1,701.047) |
| history_c8 | 346.663 (338.717~351.924) | 339.883 (333.100~342.757) | 4,621.945 (4,124.308~5,814.459) |
| send_c1 | 17.412 (16.097~17.982) | 18.395 (15.576~18.991) | 35.206 (27.108~40.827) |
| send_c4 | 25.143 (20.138~25.345) | 25.223 (23.995~25.691) | 36.480 (36.470~38.669) |

### 서버 최대 RSS (MiB)

| 작업 | Go GC 켬 | Go GC 끔 | Rust |
|---|---:|---:|---:|
| login_c1 | 14.891 (14.363~15.312) | 17.367 (17.320~17.379) | 6.535 (6.504~6.590) |
| history_c1 | 14.961 (14.934~15.129) | 51.941 (51.930~52.074) | 6.516 (6.516~6.531) |
| history_c8 | 15.641 (15.371~15.758) | 108.520 (108.207~109.383) | 7.160 (7.145~7.176) |
| send_c1 | 14.008 (13.527~14.020) | 13.938 (13.910~14.543) | 6.820 (6.816~6.871) |
| send_c4 | 15.559 (15.004~15.613) | 18.129 (17.648~18.293) | 7.020 (7.020~7.098) |

VmHWM으로 읽은 프로세스 전체 최대 RSS이며 초기화·warmup을 포함합니다. CPU 시간은 /proc tick 단위이므로 정밀 CPU 프로파일이 아닙니다. GC trace는 측정 구간만 읽어 첫째·셋째 clock phase를 각각 STW로 합산합니다. HTTP raw JSON의 gc_stw_max_us는 한 STW phase의 최댓값입니다. Rust의 GC 관련 0 값은 tracing GC가 없다는 표시이며 allocator나 스케줄링 pause를 측정한 값이 아닙니다.

동시 8 조회의 Go GC 켬/끔 처리량은 비슷하지만 Rust가 크게 높았습니다. Go는 database/sql 기본 idle 연결 2개를 유지하고 Rust는 별도 풀(max 10)을 사용하므로 연결 재사용 정책이 중요한 혼입 요인입니다. 이것이 얼마만큼 기여했는지는 별도 통제 실험을 하지 않았습니다. [Go의 기본 idle 연결 수](https://github.com/golang/go/blob/go1.25.5/src/database/sql/sql.go). send의 약 2배 개선 역시 트랜잭션 수와 DB fsync 비용 영향을 포함합니다. 특히 send_c4 p99는 Go 403.89ms, Rust 395.76ms로 p50만큼 좋아지지 않았고 Go GC off가 368.09ms로 가장 낮았습니다.

### GC를 끄면 왜 항상 빨라지지 않는가

GC 제거의 장점인 tracing/mark/일부 STW 비용 제거는 유효합니다. 그러나 동일 Go 바이너리의 GC off는 해제되지 않는 객체가 계속 쌓여 메모리 사용 패턴도 바뀝니다. 캐시·페이지 할당·공유 VM 노이즈 등도 가능성이 있으며, 이 실험은 그 원인을 프로파일링하지 않았습니다. 따라서 결과가 느려졌다는 이유로 GC가 공짜라고 결론내리지 않습니다. Rust의 결과에는 소유권 기반 해제와 다른 라이브러리·구현의 영향이 함께 들어갑니다. 이번 실험에서 GC가 주 병목이라는 근거는 얻지 못했습니다. Go는 요청 로그와 gctrace를 기록하고 Rust는 warn 로그만 기록하므로 로그 비용도 기존 구현 비교에 포함됩니다. DB CPU 제한에 따른 throttling과 trial 순서별 WAL/checkpoint 상태는 별도 측정하지 않았습니다. API 3회는 적은 표본이며 실행 순서를 회전했어도 이 영향을 완전히 제거하지 못합니다.

## 작은 작업 microbenchmark (ns/op)

| 작업 | Go GC 켬 | Go GC 끔 | Rust |
|---|---:|---:|---:|
| json | 178.248 (158.792~389.744) | 219.074 (183.029~225.980) | 102.388 (94.216~120.374) |
| form | 346.645 (291.840~369.026) | 456.592 (402.866~647.210) | 244.559 (199.206~262.314) |

각 case 500,000회. 메시지 6개 필드를 직렬화하거나 URL-encoded nick/color/msg를 파싱합니다. 원본 경로에서 사용하는 형태를 작은 adapter로 옮긴 것으로 실제 HTTP 핸들러 전체가 아닙니다. Go ParseQuery는 다중값 slice, Rust는 첫 값 HashMap이므로 결과 의미가 같은 고유 키 입력을 비교합니다. Rust는 serde_json/url, Go는 표준 라이브러리를 사용합니다. JSON 약 76ns, form 약 102ns 절약을 사용자가 느끼는 API 지연과 동일시하지 않습니다.

## 재현

원본 커밋·툴체인·환경은 [environment.json](benchmarks/results/environment.json), 측정값은 [HTTP JSON](benchmarks/results/http.json), [micro JSON](benchmarks/results/micro.json)에 있습니다. 테스트 코드와 의존성 lock도 함께 보관합니다. Go 원본은 해당 커밋에서 backend를 빌드하고 Rust는 저장소 루트에서 `cargo build --locked --release`합니다.

```sh
bash docs/benchmarks/build.sh
python3 docs/benchmarks/run_micro.py
python3 docs/benchmarks/summarize.py docs/benchmarks/results/micro.json
```

HTTP는 폐기 가능한 전용 PostgreSQL/NATS를 같은 호스트에 준비합니다. PostgreSQL user `rustalk_test`, password `disposable-test-only`; runner가 **rustalk_test_benchmark** DB를 만들고 매 trial 테이블을 TRUNCATE합니다. 실제 서비스 DB로 실행하면 안 됩니다. psycopg[binary]==3.2.12가 필요합니다. 측정용 Go 서버와 클라이언트는 같은 머신에서 실행하며 8080 포트가 비어 있어야 합니다. 원본 Go 서버가 8080을 고정 사용합니다.

```sh
BENCH_ALLOW_RESET=1 BENCH_DEPENDENCY_HOST=<전용-테스트-호스트> \
RUSTALK_SOURCE="$PWD" GOTALK_BINARY=/path/to/gotalk-reference \
python3 -u docs/benchmarks/run_http.py /tmp/rustalk-benchmark-results
python3 docs/benchmarks/summarize.py /tmp/rustalk-benchmark-results/http.json
```

재현 시 CPU 번호는 실행기에서 환경에 맞게 바꾸고, 새로운 결과는 기존 공개 기록과 구분해 저장합니다. 재현 로그는 로컬 호스트 이름을 포함할 수 있으므로 공개하지 않습니다. 설정 변화·버전 변화·부하에 따라 결과가 달라집니다. 이 기록은 개인 프로젝트의 재현 가능한 비교이며 장기 soak, 성능 보장 또는 통계적 유의성 검정은 아닙니다.
