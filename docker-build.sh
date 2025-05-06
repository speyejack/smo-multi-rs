#!/bin/bash
set -euo pipefail

if [[ "$#" == "0" ]] || [[ "$#" > "1" ]] || ! [[ "$1" =~ ^(all|x64|arm64|win64)$ ]] ; then
  echo "Usage: docker-build.sh {all|x64|arm64|win64}"
  exit 1
fi

DIR=$(dirname "$(realpath $0)")
cd "$DIR"

mkdir -p ./bin/

declare -A archs=(
  ["x64"]="x86_64-unknown-linux-gnu"
  ["arm64"]="aarch64-unknown-linux-gnu"
  ["win64"]="x86_64-pc-windows-gnu"
)

sock="//var/run/docker.sock"

for sub in "${!archs[@]}" ; do
  arch="${archs[$sub]}"

  if [[ "$1" != "all" ]] && [[ "$1" != "$sub" ]] ; then
    continue
  fi

  docker  run               \
    --rm                    \
    -v $sock:$sock          \
    -v "/$DIR/"://app/      \
    -w //app/               \
    ghcr.io/cross-rs/cross  \
      cross build           \
        --target $arch      \
        --release           \
        --bin smo-rs        \
  ;

  filename="Server"
  ext=""
  if   [[ "$sub" == "arm"   ]] ; then filename="Server.arm";
  elif [[ "$sub" == "arm64" ]] ; then filename="Server.arm64";
  elif [[ "$sub" == "win64" ]] ; then filename="Server.exe"; ext=".exe"
  fi

  cp ./target/$arch/release/smo-rs$ext ./bin/$filename
done
