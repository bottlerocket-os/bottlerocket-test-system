#!/usr/bin/env bash
set -euo pipefail

BIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
KIND_VERSION=v0.11.1
KIND_SHA256_SUM_DARWIN_AMD64="432bef555a70e9360b44661c759658265b9eaaf7f75f1beec4c4d1e6bbf97ce3"
KIND_SHA256_SUM_DARWIN_ARM64="4f019c578600c087908ac59dd0c4ce1791574f153a70608adb372d5abc58cd47"
KIND_SHA256_SUM_LINUX_AMD64="949f81b3c30ca03a3d4effdecda04f100fa3edc07a28b19400f72ede7c5f0491"
KIND_SHA256_SUM_LINUX_ARM64="320c992ada56292ec5e12b0b85f5dfc60045a6ffcdfaf6ad3f5a554e40ef0235"

usage() {
  cat >&2 <<EOF
${0##*/}

Downloads the kind binary to this repo's bin directory
if it does not already exist.

Example: ./${0##*/} --goarch arm64 --platform linux

Required:
     --goarch The architecture, either arm64 or amd64
     --platform The OS flavor, either darwin or linux
EOF
}

required_arg() {
  local arg="${1:?}"
  local value="${2}"
  if [ -z "${value}" ]; then
    echo "ERROR: ${arg} is required" >&2
    usage
    exit 2
  fi
}


parse_args() {
  while [ ${#} -gt 0 ] ; do
    case "${1}" in
      --goarch ) shift; GOARCH="${1}" ;;
      --platform ) shift; PLATFORM="${1}" ;;
      --help ) usage; exit 0 ;;
      *)
        log ERROR "Unknown argument: ${1}" >&2
        usage
        exit 2
        ;;
    esac
    shift
  done
  # Required arguments
  required_arg "--goarch" "${GOARCH}"
  required_arg "--platform" "${PLATFORM}"
}

parse_args "${@}"

case "${PLATFORM}" in
   darwin|linux) ;;
   *)
      echo "Invalid --platform value '${PLATFORM}', expected 'darwin' or 'linux'" >&2
      usage
      exit 1
      ;;
esac

case "${GOARCH}" in
   amd64|arm64) ;;
   *)
      echo "Invalid --goarch value '${GOARCH}', expected 'amd64' or 'arm64'" >&2
      usage
      exit 1
      ;;
esac

URL="https://kind.sigs.k8s.io/dl/${KIND_VERSION}/kind-${PLATFORM}-${GOARCH}"

cleanup() {
    ret="${?}"
    [ "${ret}" -ne 0 ] && rm -rf "${BIN_DIR}/kind"
    exit "${ret}"
}

trap cleanup EXIT

if [ ! -f "${BIN_DIR}/kind" ]; then
  echo "Downloading kind ${KIND_VERSION}"
  curl -Lo "${BIN_DIR}/kind" "${URL}"
  chmod +x "${BIN_DIR}/kind"
else
  echo "Kind binary found, not downloading"
fi

# set KIND_SHA256_SUM to the correct sha sum
KIND_SHA256_SUM="KIND_SHA256_SUM_$(echo "${PLATFORM}" |  tr '[:lower:]' '[:upper:]')_$(echo "${GOARCH}" |  tr '[:lower:]' '[:upper:]')"
KIND_SHA256_SUM="${!KIND_SHA256_SUM}"

echo "Checking kind binary hash sum"
if ! echo "${KIND_SHA256_SUM} ${BIN_DIR}/kind" | sha256sum -c ; then
  echo "ERROR: hash sum was incorrect, deleting the downloaded binary" >&2
  exit 1
fi

echo "Testing kind binary"
"${BIN_DIR}/kind" version
