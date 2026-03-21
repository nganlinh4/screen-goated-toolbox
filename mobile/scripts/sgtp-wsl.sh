#!/usr/bin/env bash
set -euo pipefail

show_usage() {
    cat <<'EOF'
sgtp-wsl
  WSL wrapper for the Windows Android toolchain in this repo.

Usage:
  sgtp-wsl build
  sgtp-wsl install [--skip-build]
  sgtp-wsl run [--skip-build]
  sgtp-wsl gradle <gradle args...>
  sgtp-wsl <sgtp action> [args...]

Examples:
  sgtp-wsl build
  sgtp-wsl install
  sgtp-wsl run
  sgtp-wsl gradle :androidApp:testFullDebugUnitTest --console=plain
  sgtp-wsl status
EOF
}

require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "Missing required command: $command_name" >&2
        exit 1
    fi
}

resolve_script_path() {
    local source_path="${BASH_SOURCE[0]}"
    while [ -h "$source_path" ]; do
        local source_dir
        source_dir="$(cd -P "$(dirname "$source_path")" && pwd)"
        source_path="$(readlink "$source_path")"
        [[ "$source_path" != /* ]] && source_path="$source_dir/$source_path"
    done
    cd -P "$(dirname "$source_path")" && pwd
}

find_repo_root() {
    if [ -n "${SGT_REPO_ROOT:-}" ] && [ -d "${SGT_REPO_ROOT}/mobile" ]; then
        printf '%s\n' "$SGT_REPO_ROOT"
        return
    fi

    local script_dir
    script_dir="$(resolve_script_path)"
    local repo_root
    repo_root="$(cd "$script_dir/../.." && pwd)"
    if [ -d "$repo_root/mobile" ]; then
        printf '%s\n' "$repo_root"
        return
    fi

    local current_dir="$PWD"
    while [ "$current_dir" != "/" ]; do
        if [ -d "$current_dir/mobile" ] && [ -f "$current_dir/.claude/CLAUDE.md" ]; then
            printf '%s\n' "$current_dir"
            return
        fi
        current_dir="$(dirname "$current_dir")"
    done

    echo "Could not locate the repo root. Set SGT_REPO_ROOT." >&2
    exit 1
}

windows_path_exists() {
    local windows_path="$1"
    local escaped_path="${windows_path//\\/\\\\}"
    cmd.exe /c "if exist $escaped_path (exit 0) else (exit 1)" >/dev/null 2>&1
}

pick_java_home() {
    local candidates=(
        'C:\Users\user\scoop\apps\temurin17-jdk\17.0.18-8'
        'C:\Users\user\scoop\apps\temurin17-jdk\current'
        'C:\Users\user\AppData\Local\Programs\Microsoft\jdk-17.0.10.7-hotspot'
    )
    local candidate
    for candidate in "${candidates[@]}"; do
        if windows_path_exists "${candidate}\\bin\\java.exe"; then
            printf '%s\n' "$candidate"
            return
        fi
    done
    echo "Could not find a usable Windows JDK. Update pick_java_home in mobile/scripts/sgtp-wsl.sh." >&2
    exit 1
}

pick_android_sdk() {
    local candidates=(
        'C:\Users\user\android-sdk'
        'C:\Users\user\AppData\Local\Android\Sdk'
    )
    local candidate
    for candidate in "${candidates[@]}"; do
        if windows_path_exists "${candidate}\\platform-tools\\adb.exe"; then
            printf '%s\n' "$candidate"
            return
        fi
    done
    echo "Could not find a usable Windows Android SDK. Update pick_android_sdk in mobile/scripts/sgtp-wsl.sh." >&2
    exit 1
}

run_windows_powershell() {
    local script_body="$1"
    local temp_ps1
    temp_ps1="$(mktemp --suffix=.ps1)"
    cat >"$temp_ps1" <<EOF
\$ErrorActionPreference = 'Stop'
$script_body
EOF
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$temp_ps1")"
    local status=$?
    rm -f "$temp_ps1"
    return $status
}

run_windows_gradle() {
    local repo_root="$1"
    shift
    local java_home
    java_home="$(pick_java_home)"
    local android_sdk
    android_sdk="$(pick_android_sdk)"
    local repo_root_windows
    repo_root_windows="$(wslpath -w "$repo_root")"

    local gradle_args=()
    local argument
    for argument in "$@"; do
        gradle_args+=("'${argument//\'/\'\'}'")
    done
    local gradle_args_csv=""
    if [ ${#gradle_args[@]} -gt 0 ]; then
        local old_ifs="$IFS"
        IFS=", "
        gradle_args_csv="${gradle_args[*]}"
        IFS="$old_ifs"
    fi

    run_windows_powershell "
\$env:JAVA_HOME = '$java_home'
\$env:ANDROID_HOME = '$android_sdk'
\$env:ANDROID_SDK_ROOT = '$android_sdk'
Set-Location '$repo_root_windows\\mobile'
& .\\gradlew.bat @($gradle_args_csv)
"
}

run_sgtp_ps1() {
    local repo_root="$1"
    shift
    local script_windows
    script_windows="$(wslpath -w "$repo_root/mobile/scripts/sgtp.ps1")"
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$script_windows" "$@"
}

main() {
    require_command powershell.exe
    require_command cmd.exe
    require_command wslpath

    local repo_root
    repo_root="$(find_repo_root)"

    local action="${1:-run}"
    if [ $# -gt 0 ]; then
        shift
    fi

    case "$action" in
        help|-h|--help)
            show_usage
            ;;
        build)
            run_windows_gradle "$repo_root" :androidApp:assembleFullDebug --console=plain
            ;;
        gradle)
            if [ $# -eq 0 ]; then
                echo "sgtp-wsl gradle requires at least one Gradle argument." >&2
                exit 1
            fi
            run_windows_gradle "$repo_root" "$@"
            ;;
        install)
            if [ "${1:-}" != "--skip-build" ]; then
                run_windows_gradle "$repo_root" :androidApp:assembleFullDebug --console=plain
            elif [ $# -gt 0 ]; then
                shift
            fi
            run_sgtp_ps1 "$repo_root" install "$@"
            ;;
        run)
            if [ "${1:-}" != "--skip-build" ]; then
                run_windows_gradle "$repo_root" :androidApp:assembleFullDebug --console=plain
            elif [ $# -gt 0 ]; then
                shift
            fi
            run_sgtp_ps1 "$repo_root" run "$@"
            ;;
        status|connect|pair|enable-fixed-port|launch|logcat|logcat-all|disconnect)
            run_sgtp_ps1 "$repo_root" "$action" "$@"
            ;;
        *)
            echo "Unknown action: $action" >&2
            show_usage >&2
            exit 1
            ;;
    esac
}

main "$@"
