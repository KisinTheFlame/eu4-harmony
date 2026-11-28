![Harmony for Europa Universalis IV](docs/images/banner.jpg)

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | 한국어

**Europa Universalis IV를 내부에서부터 더 쾌적하게.** 선명한 한국어·중국어·일본어 글꼴, 재시작 없는 메인 메뉴 복귀, 후반 성능 향상, 엔진 버그 수정, 그리고 더 강력한 모드 확장 기반을 게임 본래의 규칙은 그대로 둔 채 제공합니다.

> 바로가기: [플레이어 안내](#플레이어-안내) · [모드 제작자 안내](#모드-제작자-안내) · [기여자 안내](#기여자-안내)

---

## 플레이어 안내

### 바닐라 EU4와의 비교

| | 바닐라 게임 | Harmony 적용 후 |
| --- | :-: | :-: |
| 시스템 글꼴과 실시간 미리보기로 선명한 한국어·중국어·일본어 표시 (구형 2바이트 패치·비트맵 글꼴 모드 불필요) | ✗ | ✓ |
| 재시작 없이 몇 초 만에 메인 메뉴로 복귀 | ✗ | ✓ |
| 철인 모드에서도 게임 중 다른 저장 파일 바로 불러오기 | ✗ | ✓ |
| 5배속에서도 부드러운 지도 이동·확대/축소 및 후반 끊김 완화 | ✗ | ✓ |
| 저장 시 화면 멈춤 시간을 약 3분의 1로 단축 | ✗ | ✓ |
| 게임 시작, 불러오기 및 후반 진행 속도 향상 | ✗ | ✓ |
| 후계자 이름 중복, 복귀 시 메모리 누수, 종교 개혁 중심지 충돌 수정 | ✗ | ✓ |
| 배경음악 음량 평준화 및 청감에 맞는 자연스러운 볼륨 슬라이더 | ✗ | ✓ |
| 선택 사항: 비철인 모드 또는 모드 적용 상태에서도 도전과제 달성 | ✗ | ✓ |
| 투명한 권한 표시와 함께 게임 기본 스타일 UI·편의 자동화 모드 지원 | ✗ | ✓ |
| **게임 규칙 및 바닐라 데이터 변경 없음** | ✓ | ✓ |

게임 번역 텍스트는 기존처럼 한국어 패치 모드가 담당합니다. 기존 2바이트 패치용 번역 모드와 해당 저장 파일도 그대로 사용할 수 있습니다. Harmony는 **Steam Windows 버전 EU4 1.37.5** 싱글플레이 전용입니다(멀티플레이 지원은 준비 중).

### 설치 방법

**중요: 구독만으로는 Harmony가 적용되지 않습니다.** 최초 1회 파일 1개(`version.dll`)를 게임 폴더에 복사해 주세요:

1. Steam 창작마당에서 **Harmony**를 구독합니다.
2. Steam에서 **Europa Universalis IV** 우클릭 → **관리 → 로컬 파일 보기**로 게임 폴더를 엽니다(창을 유지하세요).
3. Paradox 런처 → **모드 라이브러리** → **Harmony** → 오른쪽 **… → 폴더에서 보기**를 클릭합니다. (아직 보이지 않으면 Steam을 재시작하거나, EU4가 설치된 Steam 라이브러리의 `steamapps\workshop\content\236850\<id>\` 폴더를 직접 여세요.)
4. Harmony 폴더의 `version.dll`을 게임 폴더의 `eu4.exe` 옆에 복사합니다. (확장자가 숨겨져 있으면 `version`, `eu4`로 표시됩니다. 기존 2바이트 패치 등의 `version.dll`이 이미 있다면 **덮어쓰기**를 선택하거나, 필요시 먼저 백업해 두세요.)

평소처럼 Paradox 런처에서 게임을 실행하면 됩니다. Harmony 자체는 플레이세트에 추가할 필요가 없으며, Harmony 기반 모드만 플레이세트에서 활성화하면 됩니다. 적용 후 메인 메뉴와 게임 내 메뉴 오른쪽 아래에 **Harmony v…**, **모드 목록**, **설정**이 표시됩니다.

Steam이 Harmony를 자동으로 최신 상태로 유지합니다. 새 `version.dll`이 포함된 업데이트 시 시작할 때 복사본을 자동 갱신하고 게임을 재시작합니다.

#### 수동 설치 (GitHub Releases)

[GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases)에서 `eu4-harmony-<version>-windows-x64.zip`을 받아 게임을 닫은 뒤 `eu4.exe`가 있는 게임 폴더에 압축 해제할 수도 있습니다. 로컬 설치본은 창작마당보다 우선하므로 자동 업데이트되지 않습니다. 창작마당 자동 업데이트로 돌아가려면 게임 폴더의 `eu4-harmony\` 안 `eu4_harmony_core.dll`, `eu4_harmony_crash_dumps.dll`, `fonts\`를 삭제하세요. 일반 플레이어는 CLI 압축 파일(`eu4-harmony-cli-*.zip`)이 필요하지 않습니다.

#### 제거 방법

Harmony 구독을 취소하고 게임 폴더의 `version.dll`을 삭제하세요(기존 백업이 있다면 복원). **Harmony 설정 → 정보**에서 Windows 충돌 덤프를 활성화했다면 먼저 해당 화면에서 **비활성화** 버튼을 눌러 정리해 주세요.

### 자주 묻는 질문 (FAQ)

- **구독했는데 메인 메뉴에 Harmony가 보이지 않아요.**
  구독만으로는 적용되지 않습니다. 위 4단계대로 `version.dll`이 `eu4.exe` 옆에 복사되어 있는지, Steam Windows 1.37.5 버전인지 확인해 주세요.
- **백신이 `version.dll`이나 `eu4_harmony_core.dll`을 경고합니다. 안전한가요?**
  Harmony는 MIT 라이선스의 완전한 오픈소스이며, 게임 프로세스에 로드되는 특성상 간혹 휴리스틱 오탐이 발생할 수 있습니다. 소스 코드를 직접 확인하거나 EU4 폴더를 예외로 추가하거나 직접 빌드할 수 있습니다.
- **기존 2바이트 패치나 글꼴 모드에서 어떻게 옮기나요?**
  기존 `version.dll`을 Harmony 파일로 교체하고, 구형 비트맵 글꼴 모드는 비활성화하세요(번역 텍스트 모드는 그대로 유지). 기존 저장 파일도 바로 불러올 수 있습니다.
- **멀티플레이를 지원하나요?**
  멀티플레이 지원은 준비 중입니다. 현재는 싱글플레이 전용이며 메인 메뉴의 멀티플레이 버튼이 비활성화됩니다. 멀티플레이를 하려면 Harmony 구독을 취소하거나 게임 폴더의 `version.dll`을 잠시 밖으로 옮겨 주세요.
- **나중에 Harmony를 제거해도 저장 파일을 쓸 수 있나요?**
  Harmony 본체만 사용했다면 저장 구조는 손상되지 않지만, 게임 중 직접 입력한 한글 이름 등은 UTF-8로 저장되어 제거 시 글자가 깨져 보입니다. 자체 데이터를 저장 파일에 기록하는 Harmony 모드를 사용한 경우에는 Harmony 없이 불러오는 것이 보장되지 않습니다.
- **소리가 작아졌거나 비철인 도전과제가 기존 세이브에 적용되지 않아요.**
  볼륨 슬라이더를 청감에 맞는 제곱 곡선으로 개선해 100 미만 값이 이전보다 작게 들릴 수 있습니다(한 번 다시 조절해 주세요). 비철인 도전과제는 새 게임을 시작하기 전에 활성화해야 합니다([자세히 보기](docs/game/countries/achievements.md#each-achievements-script-conditions)).

### 문제 해결

메인 메뉴 또는 게임 내 메뉴에서 **Harmony 설정 → 정보**로 이동해 **문제 해결 파일을 바탕 화면에 저장**을 클릭하면 로그, 최근 충돌 기록(큰 덤프는 자동 경량화), 설정 및 플레이세트(저장 파일 제외, 24 MiB 미만)가 하나의 zip으로 저장됩니다. 정보 페이지의 GitHub Issues 링크를 클릭해 해당 파일을 첨부해 제보해 주세요.

로그는 `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\`에 저장됩니다. 자세한 내용은 [docs/logs.md](docs/logs.md)를 참고하세요.

---

## 모드 제작자 안내

### Harmony 모드 만들기

Harmony 기반 모드는 일반 Paradox 모드에 `eu4-harmony/` 폴더를 추가한 형태입니다. Lua 확장은 메모리 주소에 직접 접근하지 않고 안전한 기능(capability) 인터페이스를 통해 게임을 읽고 조작합니다:

```lua
-- eu4-harmony/lua/main.lua: eu4-harmony/ui/에 선언한 버튼을
-- 아군 부대 선택 시에만 군대 패널에 표시하는 예시
return {
  on_frame = function(game)
    game.ui.button.visible = game.army.selected() ~= nil
  end,

  on_button = function(game)
    log("army " .. tostring(game.army.selected()) .. " clicked")
  end,
}
```

[GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases)에서 `eu4-harmony.exe`를 다운로드해 사용합니다:

```powershell
eu4-harmony new my-mod --name "My Mod"   # UI·현지화·편집기 타입 정의가 포함된 시작 모드 생성
cd my-mod
eu4-harmony check                         # 프레임워크와 동일하게 전체 파일 검사
eu4-harmony test                          # 가상 게임 환경에서 eu4-harmony\tests\*.lua 실행
eu4-harmony link                          # 런처에 등록 후 플레이세트에서 활성화
eu4-harmony reload --watch                # 게임 실행 중 파일 변경 시 자동 핫 리로드
eu4-harmony types                         # CLI 업데이트 후 LuaLS 타입 정의 재생성
eu4-harmony package                       # 창작마당 업로드용 깨끗한 폴더 생성
```

민감한 권한(`clock`, `files`, `network`)은 `manifest.toml`에 선언해야 하며 **Harmony 모드** 창에서 플레이어에게 투명하게 공개됩니다.

추가 문서:

- `eu4-harmony wiki`: 전체 기능 사양, 핸들러/전역 함수(`wiki lua`), 매니페스트 형식(`wiki manifest`). 게임 불필요.
- `eu4-harmony skills`: 작업별 단계 가이드(`skills mod`, `skills ui`, `skills testing` 등).
- [cli/skills/mod.md](cli/skills/mod.md): 확장 동작 방식, 오류 처리, 시간/메모리 제한 및 저장 데이터.

### 원격 제어 및 자동화

프로그램이나 AI가 `eu4-harmony game <capability> [<arguments>]`로 게임을 플레이어처럼 조작하고 JSON 응답을 받을 수 있습니다:

```powershell
eu4-harmony start --hidden                 # 백그라운드 숨김 실행 (Steam 실행 필요)
eu4-harmony game menu start FRA            # 프랑스로 새 게임 시작
eu4-harmony game time advance 30           # 30일 진행 (이벤트 발생 시 조기 정지)
eu4-harmony game question list             # 대기 중인 이벤트/질문 목록 조회
eu4-harmony game army move 545460846646 185
eu4-harmony game screenshot --out shot.png
eu4-harmony game menu return               # 메인 메뉴로 복귀
```

관전 모드(국가 태그 없이 `menu start`)에서는 `session view FRA` / `session leave_view`로 시점을 전환할 수 있습니다([docs/game/countries/countries.md](docs/game/countries/countries.md)). 원격 제어는 로컬 Windows 사용자에게만 열려 있습니다. 자세한 내용은 `eu4-harmony wiki remote`를 참고하세요.

---

## 기여자 안내

### 문서

[docs/README.md](docs/README.md)에서 게임 내부 로직, 개발 도구, ADR 및 조사 근거를 확인할 수 있습니다. 용어집은 [GLOSSARY.md](GLOSSARY.md), 남은 작업은 [docs/plans/README.md](docs/plans/README.md)에 정리되어 있습니다.

### 소스에서 빌드

Windows 10 (1903+) x64, [rustup](https://rustup.rs/)([rust-toolchain.toml](rust-toolchain.toml) 자동 인식), Visual Studio C++ 빌드 도구가 필요합니다. EU4 1.37.5 설치본은 프레임워크 실행, 오프라인 의존 지점 검사, 테스트 게임 실행 시에만 필요합니다.

```powershell
cargo build --release
cargo test
cargo xtask offline-check          # EU4 1.37.5 필요 (Steam 또는 EU4_GAME_DIR)
cargo xtask release --no-workshop  # SteamCMD 없이 GitHub 릴리스 ZIP 생성
```

로컬 격리 테스트 게임 및 시나리오 검증(Steam 실행, EU4 1.37.5 전체 DLC 기준):

```powershell
cargo xtask install                # 게임 종료 후 최초 1회 실행
cargo xtask saves fetch            # 공용 테스트 저장 파일 다운로드 및 검증 (최초 1회)
cargo xtask scenario sweep --owner <name>
```

빌드, 릴리스, 창작마당 업로드, 격리 테스트 게임 절차는 [docs/dev-tools.md](docs/dev-tools.md)에 있으며, Pull Request를 열기 전에 [AGENTS.md](AGENTS.md)를 먼저 읽어 주세요. 모든 변경 사항은 Pull Request를 통해 `master`에 병합됩니다.

### 라이선스

[MIT](LICENSE). 이 저장소의 코드와 문서에만 적용되며 게임 본체는 포함되지 않습니다. 모드 제작에는 Paradox의 모드 이용 약관이 적용됩니다.
