#!/bin/sh
# Build the Android shell: the JNI library for each ABI the app ships, then the APK.
#
# The NDK is discovered under the SDK; a checkout with a different NDK path only changes this
# lookup. The Rust core is statically linked into libdrawkit_jni.so, so the APK carries its own
# painting engine. arm64 is the device ABI; x86_64 is what an emulator on an Intel host runs.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
sdk=${ANDROID_HOME:-$HOME/Library/Android/sdk}
ndk=$(ls -d "$sdk"/ndk/*/ 2>/dev/null | sort -V | tail -1)
if [ -z "$ndk" ]; then
    echo "no NDK under $sdk/ndk; install one with 'android sdk install ndk/29.0.14206865'" >&2
    exit 1
fi
ndk=${ndk%/}
case "$(uname -s)" in
    Darwin) ndk_host=darwin-x86_64 ;;
    Linux) ndk_host=linux-x86_64 ;;
    *) echo "unsupported host for the Android NDK" >&2; exit 1 ;;
esac
bin="$ndk/toolchains/llvm/prebuilt/$ndk_host/bin"

if [ -z "${JAVA_HOME:-}" ]; then
    for candidate in "/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
                     "$HOME/Library/Java/JavaVirtualMachines"/*/Contents/Home; do
        if [ -x "$candidate/bin/java" ]; then
            export JAVA_HOME="$candidate"
            break
        fi
    done
fi

cd "$root"

# tract-linalg compiles small C kernels through cc-rs, and cc-rs looks for a plain
# `<triple>-clang` that the NDK does not ship; point it at the API-level wrapper.
build_abi() {
    abi=$1
    target=$2
    # Cargo names the linker variable after the triple with dashes turned into underscores.
    upper=$(printf '%s' "$target" | tr 'a-z-' 'A-Z_')
    under=$(printf '%s' "$target" | tr '-' '_')
    linker="$bin/${target}26-clang"
    env \
        "CARGO_TARGET_${upper}_LINKER=$linker" \
        "CC_${under}=$linker" \
        "CXX_${under}=$bin/${target}26-clang++" \
        "AR_${under}=$bin/llvm-ar" \
        cargo build --release --target "$target" -p drawkit-jni
    jni="$root/app-android/app/src/main/jniLibs/$abi"
    mkdir -p "$jni"
    # A release build carries its symbols, which is most of a 45 MB library. The APK needs none.
    "$bin/llvm-strip" --strip-unneeded \
        -o "$jni/libdrawkit_jni.so" "target/$target/release/libdrawkit_jni.so"
}

build_abi arm64-v8a aarch64-linux-android
build_abi x86_64 x86_64-linux-android

# `release` builds a non-debuggable APK, signed with DRAWKIT_KEYSTORE when the environment has
# one and with the debug key otherwise. The default is the debug APK CI uses.
kind=${1:-debug}
case "$kind" in
    debug) task=:app:assembleDebug ;;
    release) task=:app:assembleRelease ;;
    *) echo "usage: $0 [debug|release]" >&2; exit 2 ;;
esac

cd app-android
if [ -n "${DRAWKIT_VERSION:-}" ]; then
    ./gradlew "$task" -PdrawkitVersion="$DRAWKIT_VERSION"
else
    ./gradlew "$task"
fi
echo "built app-android/app/build/outputs/apk/$kind/app-$kind.apk"
