#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="$(awk -F'"' '/^\[llama_cpp\]/{in_section=1; next} /^\[/{in_section=0} in_section && /^version = /{print $2; exit}' local-ai.toml)"
commit="$(awk -F'"' '/^\[llama_cpp\]/{in_section=1; next} /^\[/{in_section=0} in_section && /^commit = /{print $2; exit}' local-ai.toml)"
repository="$(awk -F'"' '/^\[llama_cpp\]/{in_section=1; next} /^\[/{in_section=0} in_section && /^repository = /{print $2; exit}' local-ai.toml)"
case "$commit" in
  (*[!0123456789abcdef]*|'')
    printf 'Invalid llama.cpp commit in local-ai.toml\n' >&2
    exit 1
    ;;
esac
if [[ ${#commit} -ne 40 || -z "$version" || -z "$repository" ]]; then
  printf 'Invalid llama.cpp configuration in local-ai.toml\n' >&2
  exit 1
fi

target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
if [[ "$target_dir" != /* ]]; then
  target_dir="$repo_root/$target_dir"
fi
source="$target_dir/local-ai-src/llama.cpp"
build="$target_dir/local-ai-build"
runtime="$target_dir/release/local-runtime"
mkdir -p "$(dirname "$source")"

if [[ ! -d "$source/.git" ]]; then
  git clone --depth 1 --branch "$version" "$repository" "$source"
fi
actual_commit="$(git -C "$source" rev-parse HEAD)"
if [[ "$actual_commit" != "$commit" ]]; then
  printf 'llama.cpp checkout does not match local-ai.toml: expected %s, got %s\n' \
    "$commit" "$actual_commit" >&2
  exit 1
fi

platform="$(uname -s)"
if [[ "$platform" == Linux ]]; then
  if ! command -v glslc >/dev/null 2>&1 || ! command -v glslangValidator >/dev/null 2>&1 ||
    ! pkg-config --exists vulkan; then
    printf 'Linux Vulkan builds require libvulkan-dev, glslc, glslang-tools and spirv-headers\n' >&2
    exit 1
  fi
elif [[ "$platform" != Darwin ]]; then
  printf 'Unsupported build platform: %s\n' "$platform" >&2
  exit 1
fi

common_options=(
  -DCMAKE_BUILD_TYPE=Release
  -DBUILD_SHARED_LIBS=OFF
  -DGGML_NATIVE=OFF
  -DLLAMA_BUILD_TESTS=OFF
  -DLLAMA_BUILD_EXAMPLES=OFF
  -DLLAMA_BUILD_APP=OFF
  -DLLAMA_BUILD_TOOLS=ON
  -DLLAMA_BUILD_SERVER=ON
  -DLLAMA_BUILD_UI=OFF
  -DLLAMA_USE_PREBUILT_UI=OFF
  -DLLAMA_OPENSSL=OFF
)
cpu_build="$build/cpu"
cmake -S "$source" -B "$cpu_build" \
  "${common_options[@]}" \
  -DGGML_VULKAN=OFF
cmake --build "$cpu_build" --config Release --target llama-server --parallel 2

cpu_server="$cpu_build/bin/llama-server"
if [[ ! -f "$cpu_server" ]]; then
  printf 'The CPU llama.cpp build produced no llama-server binary at %s\n' "$cpu_server" >&2
  exit 1
fi
mkdir -p "$runtime"
install -m 755 "$cpu_server" "$runtime/llama-server"

if [[ "$platform" == Linux ]]; then
  vulkan_build="$build/vulkan"
  cmake -S "$source" -B "$vulkan_build" \
    "${common_options[@]}" \
    -DGGML_VULKAN=ON
  cmake --build "$vulkan_build" --config Release --target llama-server --parallel 2

  vulkan_server="$vulkan_build/bin/llama-server"
  if [[ ! -f "$vulkan_server" ]]; then
    printf 'The Vulkan llama.cpp build produced no llama-server binary at %s\n' "$vulkan_server" >&2
    exit 1
  fi
  mkdir -p "$runtime/vulkan"
  install -m 755 "$vulkan_server" "$runtime/vulkan/llama-server"
fi

if [[ ! -f "$source/LICENSE" ]]; then
  printf 'The pinned llama.cpp checkout has no LICENSE file\n' >&2
  exit 1
fi
install -m 644 "$source/LICENSE" "$runtime/llama.cpp-LICENSE"
