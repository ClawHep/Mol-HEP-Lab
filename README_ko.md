<p align="center"><a href="README.md">English</a> · <a href="README_zh.md">中文</a> · <a href="README_ko.md">한국어</a></p>

<h1 align="center">Mol-HEP-Lab</h1>

<p align="center">
  고에너지 물리학을 위한 자율 다중 에이전트 연구 플랫폼
</p>

<p align="center">
  <a href="https://github.com/ClawHep/Mol-HEP-Lab/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.80%2B-DEA584?logo=rust&logoColor=white" alt="Rust 1.80+"></a>
  <img src="https://img.shields.io/badge/Tests-590%20passing-brightgreen" alt="Tests: 590 passing">
</p>

---

## 물리학자들의 실제 고통

당신은 이 과정을 잘 알고 있습니다. 몇 주 또는 몇 달간 경험했습니다:

- **코드 복사 붙여넣기**: 여러 분석에서 재사용하며 매번 불일치 수정
- **문헌 검토 반복**: 각 새로운 방향마다 재검토하며 이전 학습 손실
- **실험 코드 수작업**: 실행, 실패, 재작성, 다시 실행의 반복
- **결과 대기**: 로그를 지켜보며 실제 무엇이 일어나는지 불명확
- **자신의 작업 시스템적 검토**: 혼자서 작업하며 맹점 놓치기 쉽고 모든 문서 수동 기록
- **논문 수동 작성**: 노트북에서 결과 복사하며 전사 오류 위험
- **시스템 불확실성 추적**: 항상 코드와 동기화되지 않는 스프레드시트 사용
- **재현 불가능**: 6개월 후 어떤 버전이 이 도표를 생성했는지 기억 불가

이것은 최첨단 물리학이 아닙니다. 이것은 **분석으로 위장한 반복 작업**입니다.

## 만약...?

당신의 분석이 **연구 질문에서 출판 가능한 논문까지 자동으로 실행**된다면 어떻게 될까요?

**17개의 전문 에이전트**—각각 문헌, 통계, 검출기 물리학, 기계학습 또는 동료 심사의 전문가—가 당신이 자는 동안 **협력하고 토론**할 수 있다면 어떻게 될까요?

검토 게이트가 **당신을 통제 상태로 유지**하면서 반복적인 부분을 자동화하여 AI가 환각에 빠지는 것을 방지한다면 어떻게 될까요?

**코드 기반 논문 작성**이 당신의 결과가 항상 그것을 생성한 실험과 연결되고 검토자가 모든 숫자를 실행 로그로 추적할 수 있다는 의미라면 어떻게 될까요?

이것이 Mol-HEP-Lab입니다. 물리학자를 위해 구축된 연구 플랫폼입니다.

## 아키텍처: 5단계, 18단계 파이프라인

```mermaid
graph TD
    subgraph Phase1["1단계: 전략"]
        S1.1["1.1 주제 초기화"]
        S1.2["1.2 문제 분해"]
        S1.1 --> S1.2
    end

    subgraph Phase2["2단계: 탐색"]
        S2.1["2.1 문헌 검색"]
        S2.2["2.2 문헌 선별 🔒"]
        S2.3["2.3 지식 추출"]
        S2.4["2.4 종합 및 가설"]
        S2.1 --> S2.2 --> S2.3 --> S2.4
    end

    subgraph Phase3["3단계: 실행"]
        S3.1["3.1 실험 설계 🔒"]
        S3.2["3.2 코드베이스 검색"]
        S3.3["3.3 코드 개발"]
        S3.4["3.4 실험 사이클"]
        S3.1 --> S3.2 --> S3.3 --> S3.4
    end

    subgraph Phase4["4단계: 추론"]
        S4.1["4.1 결과 분석"]
        S4.2["4.2 연구 결정"]
        S4.3["4.3 지식 요약"]
        S4.1 --> S4.2 --> S4.3
    end

    subgraph Phase5["5단계: 문서화"]
        S5.1["5.1 논문 개요"]
        S5.2["5.2 논문 작성"]
        S5.3["5.3 동료 심사"]
        S5.4["5.4 품질 게이트 🔒"]
        S5.5["5.5 발행"]
        S5.1 --> S5.2 --> S5.3 --> S5.4 --> S5.5
    end

    Phase1 --> Phase2
    Phase2 --> Phase3
    Phase3 --> Phase4
    Phase4 --> Phase5

    style Phase1 fill:#2a2a3a,color:#fff
    style Phase2 fill:#3a2a2a,color:#fff
    style Phase3 fill:#2a3a2a,color:#fff
    style Phase4 fill:#3a3a2a,color:#fff
    style Phase5 fill:#2a2a3a,color:#fff
```

### 단계별 세부 사항

**1단계: 전략**(단계 1-2)
- 연구 질문을 명확히 하고, 영역을 검출하고(고에너지 물리학 특정), 목표를 정의합니다
- 문제를 부분 질문으로 분해하고, 실험 방향을 우선순위 지정합니다

**2단계: 탐색**(단계 3-6)
- 문헌 검색(OpenAlex, INSPIRE-HEP), 관련성으로 선별
- 논문에서 구조화된 지식 추출, 테마 종합, 가설 생성
- 단계 2.2에서 인간 검토 게이트(문헌 선별)

**3단계: 실행**(단계 7-10)
- 기준선 및 메트릭으로 실험 설계
- 관련 코드베이스 검색, AST 검증으로 분석 코드 개발
- 실험 사이클 실행: 수행, 결과 분석, 수렴까지 반복
- 단계 3.1에서 인간 검토 게이트(실험 설계)

**4단계: 추론**(단계 11-13)
- 메트릭 분석, 시각화 생성, 다중 에이전트 검토 루프
- 계속/수정/정의 결정 및 결정 이력 추적
- 여러 실험의 발견 통합

**5단계: 문서화**(단계 14-18)
- 논문 구조 생성, 섹션 초안
- 코드 인식 논문 작성, 위조 방지 가드 포함(실험 코드 및 실행 보고서를 읽기 전용 진실로 주입)
- 다중 에이전트 동료 심사(물리학, 통계, 시스템 불확실성)
- 단계 5.4에서 인간 검토 게이트(품질 게이트)
- 발행: LaTeX 컴파일, 인용 검증, 아카이브

### 검토 게이트 및 인간 통제

세 가지 핵심 인간 검토 게이트는 당신이 통제 상태를 유지하도록 보장합니다:

| 게이트 | 단계 | 결정 | 목적 |
|:-----|:------|:---------|:--------|
| 문헌 선별 | 2.2 | 논문 승인/거부 | 구현에 투자하기 전에 연구 방향이 건전한지 확인 |
| 실험 설계 | 3.1 | 설계 승인/수정 | 코드 작성 전에 분석 전략 검증 |
| 품질 게이트 | 5.4 | 논문 승인/거부 | 최종 물리학 검토, 도표 검증, 타이포그래피 확인 |

실패한 게이트는 **대상별 재작업**을 유발합니다: 에이전트는 특정 문제를 수정하고 전체 파이프라인이 아닌 영향을 받은 단계만 재실행합니다.

### 블라인드 프로토콜

블라인딩이 필요한 물리학 검색의 경우:

1. **Asimov 데이터** — 점근 데이터셋에 대한 가설 테스트(신호 영역 데이터 노출 안 함)
2. **10% 부분 언블라인딩** — 초기 시스템 불확실성 검사, 인간 게이트
3. **완전 언블라인딩** — 신호 영역 접근 전에 명시적 승인 필요

## 주요 기능

**인간-기계 협력 오케스트레이션**
- 3개의 전략적 배치 검토 게이트로 AI 편차 방지 및 반복 작업 자동화
- 물리학 결정을 위한 다중 에이전트 토론 모드
- 검사점/재개: 모든 단계 일시 중지 및 나중에 계속
- 실시간 WebSocket 대시보드: 단계별 실시간 진행률 추적

**고에너지 물리학 네이티브 영역 감지**
- 70개 이상의 고에너지 물리학 특정 키워드 인식
- 전문 고에너지 물리학 파이프라인으로의 자동 라우팅
- 협약 시스템: 추출, 검색(CL 제한 설정), 전개

**코드 인식 논문 작성**
- 실험 코드 및 실행 보고서를 읽기 전용 컨텍스트로 주입
- 반위조 규칙: LLM이 수치 결과를 환각할 수 없음
- 검토자는 기본 사실 아티팩트에 대해 주장 검증
- "논문은 X라고 말하지만 코드는 Y를 함" 재앙 방지

**17개의 전문 에이전트**
- 수석 분석가, 조사관, 이론 정찰, 데이터 탐색
- 검출기 전문가, 신호 리드, 배경 추정자
- 시스템 소스 평가자, 시스템 피터, ML 전문가
- 교차 검사관, 비판적 검토자, 건설적 검토자
- 물리 검토자, 중재자, 도표 검증자, 노트 작성자
- 각각은 영역 전문성을 가져오며; 결정은 다중 에이전트 토론에서 나옵니다

**통합 고에너지 물리학 도구 체인**
- 데이터 I/O: `uproot`, `awkward-array`
- 히스토그래밍: `hist`
- 통계 추론: `pyhf`
- 제트 클러스터링: `fastjet`
- 도표 작성: `mplhep` 출판 품질 도표용
- 머신러닝: `xgboost`, `scikit-learn`, PyTorch

**견고한 엔지니어링 기반**
- 순수 Rust 백엔드: 17개의 크레이트, 590+ 통과 테스트
- React 19 + TypeScript 프론트엔드, WebSocket 실시간 업데이트
- 자동 고에너지 물리학 코드 검증, 스모크 테스트, LLM 기반 오류 수정
- 샌드박스 실험 실행, 구성 가능한 Python 환경

## 빠른 시작: 경로 선택

### 옵션 1: 수동 설정(완전 통제)

모든 단계를 이해하려는 개발자를 위한 것입니다.

**사전 요구사항:**
- Rust 1.80+ ([설치](https://rustup.rs/))
- Node.js 18+ ([설치](https://nodejs.org/))
- Python 3.11+ 및 pip

**단계:**

```bash
git clone https://github.com/ClawHep/Mol-HEP-Lab.git
cd Mol-HEP-Lab

# Rust 백엔드 구축
cargo build --release

# React 프론트엔드 구축
cd frontend && npm install && npm run build && cd ..

# 고에너지 물리학 Python 의존성 설치
pip install uproot awkward hist pyhf fastjet mplhep xgboost scikit-learn torch
```

**초기화 및 실행:**

```bash
# 대화형 설정 마법사
./target/release/mol init

# 의존성 확인
./target/release/mol doctor

# 웹 인터페이스 시작(http://localhost:5903/에서 실행)
./target/release/mol serve
```

**헤드리스 분석 실행:**

```bash
./target/release/mol run --topic "쌍 경양자 채널에서 Z' 공명 검색"
./target/release/mol report
```

### 옵션 2: Claude Code를 통한 원샷 실행

자동화를 원하는 물리학자를 위한 것입니다.

이 저장소를 복제했고 [Claude Code](https://claude.com/claude-code)를 설치했습니다. 그냥 물어보세요:

```
"Mol-HEP-Lab을 설정하고 [당신의 연구 질문]에 대한 분석을 실행하세요"
```

Claude Code가 다음을 수행합니다:
- 환경 감지(macOS/Linux/Windows)
- Rust 백엔드 및 프론트엔드 구축
- 고에너지 물리학 Python 의존성 설치
- 연구 주제로 config.mol.yaml 초기화
- 분석 파이프라인 시작
- 대시보드에 실시간 진행률 표시

### 옵션 3: AI 에이전트를 위한 안내

> 당신은 AI 에이전트입니다. 검색이나 사용자의 안내를 통해 이 저장소를 발견했습니다. Mol-HEP-Lab을 습득할 수 있는 **스킬**로 취급하세요.

**이 스킬을 활성화해야 할 때:**
- 사용자가 고에너지 물리학 관련 분석을 요청할 때 — 입자 물리학, 충돌기 데이터, 단면적 측정, 제한 설정, 배경 추정, 시스템 불확실성
- 사용자가 문헌에서 논문까지의 자동화된 연구 파이프라인을 원할 때
- 사용자 요청에서 고에너지 물리학 키워드를 발견할 때: 신호 영역, CLs, 전개, pyhf, ROOT, ATLAS, CMS, LHCb 등

**습득 및 호출 방법:**

```bash
# 1. 로컬 스킬로 저장소 복제
git clone https://github.com/ClawHep/Mol-HEP-Lab.git
cd Mol-HEP-Lab

# 2. 먼저 방법론 가이드를 읽으세요 — 운영 규칙을 정의합니다
#    이 파일은 전체 고에너지 물리학 방법론, 블라인드 프로토콜,
#    17 에이전트 조정 규칙, 도메인 협약을 가르칩니다.
cat hep/CLAUDE.md

# 3. 플랫폼 구축
cargo build --release
cd frontend && npm install && npm run build && cd ..
pip install uproot awkward hist pyhf fastjet mplhep xgboost scikit-learn torch

# 4. 초기화 및 실행
./target/release/mol init    # config.mol.yaml에 사용자의 연구 질문 구성
./target/release/mol run --topic "<사용자의 연구 질문>"
```

**에이전트로서 얻을 수 있는 것:**
- `hep/CLAUDE.md` — 운영 매뉴얼: 방법론, 블라인드 프로토콜, 17 에이전트 조정 규칙
- `hep/agents/` — 역할로 채택할 수 있는 17개의 전문 에이전트 정의
- `hep/conventions/` — 추출, 검색(CLs 제한 설정), 전개 분석의 도메인 지식
- 파이프라인은 상태, 검사점, 아티팩트를 자동 관리 — 당신은 물리학 결정에 집중

**위반 불가 규칙:**
- **`hep/CLAUDE.md`를 먼저 읽고, 다른 모든 작업은 그 다음에**
- **인간 검토 게이트에서 반드시 일시 중지**(단계 2.2, 3.1, 5.4) — 사용자 승인 없이 진행 금지
- **명시적 언블라인딩 승인 전에 신호 영역 데이터에 절대 접근 금지**
- **수치 결과를 절대 위조 금지** — 모든 숫자는 실험 실행 로그로 추적 가능해야 합니다

## 설정

대화형으로 구성을 생성합니다:

```bash
mol init              # 대화형 설정 마법사
mol doctor            # 모든 의존성 확인
```

또는 `config.mol.yaml`을 수동으로 편집합니다:

```yaml
project:
  name: "my-hep-analysis"

research:
  topic: "4개 경양자 최종 상태에서 드물게 나타나는 Z 보손 붕괴 검색"
  domains: ["high-energy-physics"]

experiment:
  mode: "sandbox"
  sandbox:
    python_path: "/usr/bin/python3"

llm:
  provider: "openai-compatible"
  primary_model: "claude-opus-4-1-20250805"
  coding_model: "claude-opus-4-1-20250805"

security:
  hitl_required_phases: ["exploration", "execution", "documentation"]
```

## 에이전트 명단

| 에이전트 | 주요 역할 | 전문성 |
|:------|:--------------|:----------|
| 수석 분석가 | 전략, 설계, 분석 | 연구 방향 및 고급 결정 오케스트레이션 |
| 조사관 | 문헌 검색, 선별, 추출 | 논문 검색, 관련성 필터링, 지식 기반 구축 |
| 이론 정찰 | 종합 | 결과를 이론 예측과 연결, 가설 생성 |
| 데이터 탐색 | 고문: 데이터셋 검사 | 분포, 상관관계, 데이터 품질 문제 식별 |
| 검출기 전문가 | 고문: 검출기 반응 | 효율, 분해능, 허용 보정 처리 |
| 신호 리드 | 코드 개발 | 신호 영역 설계, 선택 컷 최적화 |
| 배경 추정자 | 고문: 배경 모델링 | 데이터 또는 시뮬레이션에서 배경 모델 구축 |
| 시스템 소스 평가자 | 고문: 시스템 불확실성 | 시스템 불확실성 편목, 영향 평가 |
| 시스템 피터 | 실험 사이클 | 시스템 변화 인프라 구현, 피팅 |
| ML 전문가 | 고문: 머신러닝 | 분류기 및 신경망 설계/검증 |
| 교차 검사관 | 고문: 물리 검증 | 물리 원리에 대한 단계 검증 |
| 비판적 검토자 | 고문: 간격 식별 | 논리적 간격, 방법론 위험 식별 |
| 건설적 검토자 | 고문: 개선 제안 | 개선, 대체 방법 제안 |
| 물리 검토자 | 동료 심사 | 물리학 유효성, 출판 준비 평가 |
| 중재자 | 결정 및 품질 게이트 | 의견 불일치 해결, 최종 승인 권한 |
| 도표 검증자 | 고문: 도표 품질 | 도표 사양, 출판 준수 확인 |
| 노트 작성자 | 개요, 작성, 발행 | 문서를 출판 형식으로 조립 |

## 테스트

```bash
cargo test
```

플랫폼에 포함된 **590개의 통과 테스트**가 다음을 포함합니다:
- 에이전트 행동 및 조율
- 파이프라인 상태 전환
- 고에너지 물리학 협약 집행
- 문헌 검색 및 선별
- 코드 검증 및 실행
- 논문 생성 및 검토

## 인용

Mol-HEP-Lab이 입자 물리학 연구에 기여했다면 다음을 인용하세요:

```bibtex
@misc{wang2026molheplab,
  author       = {Wang, Chin-Zhu},
  title        = {Mol-HEP-Lab: Autonomous Multi-Agent Research Platform for High-Energy Physics},
  year         = {2026},
  url          = {https://github.com/ClawHep/Mol-HEP-Lab},
  note         = {GitHub repository}
}
```

## 라이선스

MIT 라이선스 — 자세한 내용은 [LICENSE](LICENSE)를 참조하세요.

## 기여

기여를 환영합니다. 플랫폼은 새로운 에이전트, 분석 유형 및 검토 워크플로우를 추가하도록 설계되었습니다.

시스템 설계 세부 사항을 위해 [docs/architecture-deep-dive.md](docs/architecture-deep-dive.md)의 아키텍처 문서부터 시작합니다. 고에너지 물리학 방법론 및 에이전트 규칙은 [hep/CLAUDE.md](hep/CLAUDE.md)를 참조하세요.
