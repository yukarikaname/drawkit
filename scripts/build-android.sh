#!/bin/sh
# Build the Android shell: the JNI library for the emulator's arm64 ABI, then the APK.
#
# The NDK is discovered under the SDK; a checkout with a different NDK path only changes this
# lookup. The Rust core is statically linked into libdrawkit_jni.so, so the APK carries its own
# painting engine.

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
linker="$ndk/toolchains/llvm/prebuilt/$ndk_host/bin/aarch64-linux-android26-clang"

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
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$linker" \
    cargo build --release --target aarch64-linux-android -p drawkit-jni

jni="$root/app-android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$jni"
cp target/aarch64-linux-android/release/libdrawkit_jni.so "$jni/"

cd app-android
./gradlew :app:assembleDebug
echo "built app-android/app/build/outputs/apk/debug/app-debug.apk"
