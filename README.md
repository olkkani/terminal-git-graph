# terminal-git-graph (`tgg`)

터미널에서 쓰는 Git 그래프 뷰어입니다. 브랜치·리모트·태그가 표시된 커밋 그래프를 보고, 커밋별 변경 파일과 diff를 확인하고, 자주 쓰는 Git 작업을 메뉴로 실행할 수 있습니다. 저장소가 아닌 폴더를 열면 하위 저장소 목록(Manygit)을 보여주고, 거기서 바로 그래프로 들어갈 수 있습니다.

<!-- TODO: 메인 스크린샷 (그래프 + 커밋 상세) -->
<!-- ![tgg 메인 화면](docs/images/main.png) -->

## 목차

- [주요 기능](#주요-기능)
- [지원 환경](#지원-환경)
- [설치 전 준비물](#설치-전-준비물)
- [설치 방법](#설치-방법)
- [사용법](#사용법)
- [herdr 단축키 설정](#herdr-단축키-설정)
- [설정 파일](#설정-파일)
- [문제 해결](#문제-해결)
- [개발](#개발)
- [라이선스](#라이선스)

## 주요 기능

- **커밋 그래프** — 전체 브랜치 / 현재 브랜치, 리모트 브랜치 표시 여부를 전환할 수 있습니다. 커밋은 300개씩 불러오며, 끝까지 스크롤하면 다음 300개를 불러옵니다.
- **커밋 상세** — 메시지, 작성자, 변경 파일, 파일별 diff. 커밋하지 않은 변경 사항도 별도 행으로 표시됩니다.
- **메뉴로 Git 작업 실행** — 체크아웃, 브랜치 생성/이름 변경/삭제, merge, rebase, cherry-pick, revert, reset(soft/mixed/hard), 태그 추가/삭제/push, push, 원격 브랜치 삭제, commit, stash, 이름/해시 복사
- **안전장치** — 로컬 브랜치 체크아웃과 Fetch를 제외한 모든 변경 작업은 실행 전에 확인을 받습니다. Git이 거부하면 Git의 메시지를 그대로 보여줍니다.
- **자동 새로고침** — 저장소의 ref나 파일이 바뀌면 스스로 갱신합니다.
- **Manygit** — 폴더 아래 저장소들의 브랜치, 변경 여부, ahead/behind를 15초마다 갱신해 보여줍니다. pull, push, fetch, 전체 fetch, commit, stash, reset, 원격 저장소 웹 페이지 열기를 지원합니다.
- **키보드 + 마우스** — 클릭, 더블 클릭, 우클릭 메뉴, 휠 스크롤
- **프라이버시** — 사용자가 직접 실행한 fetch/pull/push/원격 브랜치 삭제 외에는 네트워크 접속이 없습니다. 텔레메트리와 업데이트 확인도 없습니다.

<!-- TODO: 기능 스크린샷 (우클릭 메뉴, Manygit 목록) -->
<!-- ![메뉴](docs/images/menu.png) -->
<!-- ![Manygit](docs/images/manygit.png) -->

## 지원 환경

| 항목 | 지원 여부 | 비고 |
| --- | --- | --- |
| macOS | ✅ 지원 | 주 개발·테스트 환경 |
| Linux | ⚠️ 동작 예상 | 정식 검증은 하지 않았습니다 |
| Windows | ❌ 미지원 | |

**터미널**

| 환경 | 그래프 표시 방식 |
| --- | --- |
| Kitty 그래픽 프로토콜 지원 터미널 (Ghostty, kitty 등) | **그래픽 모드** — 안티앨리어싱된 부드러운 선과 둥근 노드 |
| 그 외 터미널, SSH 접속 | **텍스트 모드** — 박스 드로잉 문자로 그림, 모든 기능 동일 |
| tmux, zellij 내부 | 텍스트 모드 (`TMUX` / `ZELLIJ` 감지 시 자동 선택) |
| 박스 드로잉 미지원 터미널 | ASCII 문자로 대체 |
| True color 미지원 터미널 | 256색으로 대체 |

그래픽 모드는 시작할 때 터미널에 직접 물어봐서 지원 여부를 확인합니다(최대 약 450ms). 환경 변수만으로는 켜지지 않습니다. 자세한 내용은 [docs/visual-modes.md](docs/visual-modes.md)를 참고하세요.

<!-- TODO: 그래픽 모드 vs 텍스트 모드 비교 이미지 -->
<!-- ![그래픽 vs 텍스트](docs/images/modes.png) -->

## 설치 전 준비물

| 준비물 | 용도 | 확인 방법 |
| --- | --- | --- |
| **Git** | 모든 Git 작업에 사용. `PATH`에 있어야 합니다 | `git --version` |
| **Rust 툴체인** (stable) | 소스에서 빌드. [rustup](https://rustup.rs)으로 설치 | `cargo --version` |
| **Nerd Font** (권장) | ref 배지의 Git 아이콘 표시. 없으면 설정에서 끌 수 있습니다 ([설정 파일](#설정-파일)) | 터미널 폰트 설정 확인 |
| **Kitty 그래픽 지원 터미널** (선택) | 그래픽 모드를 쓰려면 필요. 없어도 텍스트 모드로 모든 기능을 쓸 수 있습니다 | [지원 환경](#지원-환경) |

herdr 단축키를 설정하려면 아래 항목이 추가로 필요합니다.

| 준비물 | 용도 | 확인 방법 |
| --- | --- | --- |
| **herdr** | 터미널 워크스페이스 매니저 | `herdr --version` |
| **jq** | 단축키 스크립트에서 JSON 파싱 | `jq --version` (macOS: `brew install jq`) |

> 미리 빌드된 바이너리를 쓰면 Rust 툴체인이 필요 없습니다. 소스에서 빌드할 때만 필요합니다.

## 설치 방법

### 1. 설치 스크립트 (권장)

macOS(Apple Silicon/Intel)와 Linux(x86_64/arm64)를 지원합니다. 최신 릴리스 바이너리를 받아 `~/.cargo/bin`에 설치합니다.

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/olkkani/terminal-git-graph/releases/latest/download/terminal-git-graph-installer.sh | sh
```

### 2. 저장소 clone 후 설치

```sh
git clone git@github.com:olkkani/terminal-git-graph.git
cd terminal-git-graph
cargo install --path .
```

### 3. (또는) clone 없이 바로 설치

```sh
cargo install --git https://github.com/olkkani/terminal-git-graph
```

`tgg` 바이너리가 `~/.cargo/bin`에 설치됩니다. 설치가 끝나면 확인해 보세요.

```sh
tgg --version
```

`command not found`가 나오면 `~/.cargo/bin`이 `PATH`에 없는 것입니다. 셸 설정 파일(`~/.zshrc` 등)에 추가하세요.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

### 업데이트 / 삭제

```sh
# 업데이트: 최신 소스를 받은 뒤 같은 명령을 다시 실행
git pull && cargo install --path . --force

# 삭제
cargo uninstall terminal-git-graph
```

## 사용법

```sh
tgg [PATH]
```

- `PATH`가 **저장소**(또는 저장소 안의 폴더)이면 → 커밋 그래프를 보여줍니다.
- `PATH`가 **일반 폴더**이면 → 그 아래 저장소 목록(Manygit)을 보여줍니다.
- `PATH`를 생략하면 현재 폴더를 사용합니다.

실행 중에는 `?`로 키 도움말, `q`로 종료할 수 있습니다.

**자주 쓰는 키**

| 키 | 동작 |
| --- | --- |
| `j` / `k` | 선택 이동 |
| `Enter` | 커밋 상세 열기 / 파일 diff 보기 |
| `m` | 선택한 항목의 메뉴 |
| `a` | 전체 브랜치 ↔ 현재 브랜치 |
| `r` | 리모트 브랜치 표시/숨김 |
| `f` | Fetch |
| `R` / `F5` | 새로고침 |
| `Esc` | 닫기 / 뒤로 |
| `q` | 종료 |

전체 단축키는 [docs/keys.md](docs/keys.md)에 있습니다.

## herdr 단축키 설정

herdr에서 단축키 하나로 **현재 pane 오른쪽에 새 pane을 열고 `tgg`를 실행**하도록 설정합니다. 새 pane은 현재 pane의 작업 폴더에서 열리며, `tgg`를 종료하면 pane도 함께 닫힙니다.

<!-- TODO: herdr에서 tgg가 열리는 모습 (스크린샷 또는 GIF) -->
<!-- ![herdr 연동](docs/images/herdr.png) -->

> 아래 예시는 단축키를 `prefix` + `g`로 지정합니다. 다른 키를 쓰려면 `key` 값만 바꾸세요.

### 1. 스크립트 만들기

`~/.config/herdr/scripts/tgg-right.sh` 파일을 만듭니다.

```sh
mkdir -p ~/.config/herdr/scripts
```

```sh
#!/bin/sh
# 현재 pane의 오른쪽에 새 pane을 열고 그 pane의 작업 폴더에서 tgg를 실행한다.
# tgg를 종료하면 pane도 닫힌다 (exec가 셸을 대체하기 때문).
pane="${HERDR_ACTIVE_PANE_ID:-$HERDR_PANE_ID}"
cwd="${HERDR_ACTIVE_PANE_CWD:-$PWD}"
new=$(herdr pane split --pane "$pane" --direction right --cwd "$cwd" --focus | jq -r '.result.pane.pane_id') || exit 1
[ -n "$new" ] && [ "$new" != null ] || exit 1
herdr pane run "$new" "exec \$HOME/.cargo/bin/tgg"
```

실행 권한을 줍니다.

```sh
chmod +x ~/.config/herdr/scripts/tgg-right.sh
```

### 2. herdr 설정에 단축키 등록

`~/.config/herdr/config.toml`에 아래 내용을 추가합니다. `command`에는 **절대 경로**를 적으세요(`<사용자명>`을 본인 계정으로 바꿉니다).

```toml
[[keys.command]]
key = "prefix+g"
type = "shell"
command = "/Users/<사용자명>/.config/herdr/scripts/tgg-right.sh"
description = "open tgg to the right of the pane"
```

`prefix`는 `[keys]` 섹션에서 지정한 herdr의 prefix 키입니다. 예를 들어 `prefix = "ctrl+space"`이면 `Ctrl+Space`를 누른 뒤 `g`를 누르면 됩니다.

### 3. 설정 다시 불러오기

```sh
herdr server reload-config
```

이제 herdr 안에서 `prefix` → `g`를 누르면 오른쪽에 `tgg`가 열립니다.

### 참고

- **그래픽 모드**: herdr 안에서 Kitty 그래픽을 쓰려면 `config.toml`의 `[experimental]` 섹션에 `kitty_graphics = true`가 필요할 수 있습니다. herdr 버전에 따라 다르니 herdr 문서를 확인하세요. 그래픽이 동작하지 않으면 `tgg`는 자동으로 텍스트 모드로 동작합니다.
- **다른 방향으로 열기**: 스크립트의 `--direction right`를 `down` 등으로 바꾸면 열리는 위치가 달라집니다.
- **`tgg` 경로**: `cargo install`로 설치했다면 `~/.cargo/bin/tgg`입니다. 다른 위치에 설치했다면 스크립트 마지막 줄을 수정하세요.

## 설정 파일

선택 사항입니다. `~/.config/terminal-git-graph/config.toml` 하나만 사용하며, 모든 키는 생략할 수 있습니다. 아래는 기본값입니다.

```toml
# Manygit이 저장소를 찾는 폴더 깊이 (1~5)
scan_depth = 3
# Manygit 목록에 각 저장소의 최신 태그 표시
show_tags = false
# 색상 테마
theme = "default"
# "auto": 터미널이 지원하면 그래픽 사용 / "text": 항상 텍스트 /
# "graphics": auto와 같지만 사용할 수 없으면 경고
renderer = "auto"
# ref 배지에 Nerd Font Git 아이콘 사용. Nerd Font가 아니면 false
nerd_font = true
```

별도의 설정 화면은 없습니다. 파일이 잘못되면 시작할 때 알려주고 기본값으로 실행합니다.

## 문제 해결

| 증상 | 해결 방법 |
| --- | --- |
| `tgg: command not found` | `~/.cargo/bin`을 `PATH`에 추가하세요 ([설치 방법](#설치-방법)) |
| `standard output is not a terminal` | 파이프·리다이렉트가 아닌 대화형 터미널에서 실행하세요 |
| ref 배지 아이콘이 `□`나 `?`로 보임 | Nerd Font를 설치하거나 `nerd_font = false`로 설정하세요 |
| 그래픽 모드가 켜지지 않음 | Kitty 그래픽 지원 터미널인지, tmux/zellij 안이 아닌지 확인하세요. `renderer = "graphics"`로 두면 실패 사유를 시작 시 알려줍니다 |
| herdr 단축키를 눌러도 반응이 없음 | `jq` 설치 여부, 스크립트 실행 권한, `command`의 절대 경로, `herdr server reload-config` 실행 여부를 확인하세요 |
| 화면이 너무 작다는 메시지 | 터미널(또는 pane) 크기를 키우세요 |

## 개발

```sh
cargo test
cargo test --release -- --ignored   # 커밋 5만 개 저장소 대상 성능 검사
cargo clippy --all-targets -- -D warnings
cargo deny check licenses
```

## 라이선스

MIT. [LICENSE](LICENSE)를 참고하세요.

herdr-git-graph와 VS Code Git Graph에서 영감을 받았으며, 두 프로젝트와 코드를 공유하지 않습니다.
