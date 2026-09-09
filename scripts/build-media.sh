#!/usr/bin/env bash
# Native, reproducible media executables. No Homebrew/shared codec libraries.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work="$root/target/media-build"
out="$root/target/bundle"
mkdir -p "$work" "$out"
rm -rf "$out/media-source"
mkdir -p "$out/media-source"
fetch() {
  local url=$1 file=$2 digest=$3
  if [[ ! -f "$work/$file" ]]; then curl --fail --location --retry 3 "$url" -o "$work/$file"; fi
  printf '%s  %s\n' "$digest" "$work/$file" | shasum -a 256 -c -
}
fetch https://ffmpeg.org/releases/ffmpeg-7.1.4.tar.xz ffmpeg-7.1.4.tar.xz 71f4aac3573ed9060489cb62526a6c7dda815ae10993789611acd7be9fa9fbf4
fetch https://codeload.github.com/mirror/x264/tar.gz/31e19f92f00c7003fa115047ce50978bc98c3a0d x264.tar.gz d053c9d86988d6bc78237ca5205865c5ddf99c98ef4cd9927eec8f6d388f6dd9
fetch https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz zlib.tar.gz 9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23
cp "$work/ffmpeg-7.1.4.tar.xz" "$work/x264.tar.gz" "$work/zlib.tar.gz" "$out/media-source/"
cp "$0" "$out/media-source/build-media.sh"
if [[ "${1:-}" == --sources-only ]]; then exit 0; fi
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64|Linux-x86_64) ;;
  *) echo "Supported build hosts: macOS arm64 and Linux x86_64" >&2; exit 1 ;;
esac
if [[ "$(uname -s)" == Darwin ]]; then
  export MACOSX_DEPLOYMENT_TARGET=11.0
  x264_host=aarch64-apple-darwin
  jobs=$(sysctl -n hw.ncpu 2>/dev/null || echo 4)
else
  x264_host=x86_64-unknown-linux-gnu
  jobs=$(getconf _NPROCESSORS_ONLN)
fi
jobs=${CERUL_BUILD_JOBS:-$jobs}
cd "$work"
# Reconfigure from clean sources so environment/toolchain changes cannot reuse objects.
rm -rf zlib-1.3.1 ffmpeg-7.1.4 x264-31e19f92f00c7003fa115047ce50978bc98c3a0d prefix
mkdir prefix
prefix="$work/prefix"
tar -xf ffmpeg-7.1.4.tar.xz
tar -xf x264.tar.gz
tar -xf zlib.tar.gz
cd zlib-1.3.1
./configure --static --prefix="$prefix"
make -j"$jobs"
make install
cd ..
cd x264-31e19f92f00c7003fa115047ce50978bc98c3a0d
./configure --host="$x264_host" --prefix="$prefix" --enable-static --disable-cli --disable-opencl --disable-asm --enable-pic
make -j"$jobs"
make install
cd ../ffmpeg-7.1.4
PKG_CONFIG_PATH="$prefix/lib/pkgconfig" ./configure --prefix="$prefix" \
  --disable-autodetect --disable-shared --enable-static --enable-gpl --enable-libx264 --enable-zlib \
  --disable-doc --disable-debug --disable-ffplay --disable-network --disable-x86asm \
  --disable-videotoolbox --disable-audiotoolbox --disable-securetransport \
  --extra-cflags="-I$prefix/include" --extra-ldflags="-L$prefix/lib" --pkg-config-flags=--static
make -j"$jobs" ffmpeg ffprobe
cp ffmpeg "$out/cerul-ffmpeg"
cp ffprobe "$out/cerul-ffprobe"
cp ../zlib-1.3.1/LICENSE "$out/media-source/zlib-LICENSE.txt"
cp COPYING.GPLv2 "$out/media-source/FFmpeg-LICENSE.txt"
cp ../x264-31e19f92f00c7003fa115047ce50978bc98c3a0d/COPYING "$out/media-source/x264-LICENSE.txt"
"$out/cerul-ffmpeg" -v error -f lavfi -i testsrc2=s=64x64:r=2 -t 1 -c:v libx264 -y "$work/smoke.mp4"
"$out/cerul-ffprobe" -v error -show_streams "$work/smoke.mp4" > "$work/smoke.txt"
if [[ "$(uname -s)" == Darwin ]]; then
  otool -L "$out/cerul-ffmpeg" "$out/cerul-ffprobe" > "$work/linkage.txt"
  if grep -E '/opt/homebrew|/usr/local|media-build' "$work/linkage.txt"; then exit 1; fi
else
  ldd "$out/cerul-ffmpeg" "$out/cerul-ffprobe" > "$work/linkage.txt"
  if grep -E 'lib(avcodec|avformat|avutil|swscale|swresample|x264)|not found' "$work/linkage.txt"; then exit 1; fi
fi
