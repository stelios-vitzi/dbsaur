#!/usr/bin/env bash
set -euo pipefail

# Keep the Ubuntu 22.04 compatibility baseline, without the hosted runner's
# Azure mirror list, which can stall on DNS failures for every package.
mirror=''
for candidate in \
  https://archive.ubuntu.com/ubuntu \
  https://mirrors.edge.kernel.org/ubuntu \
  https://mirror.math.princeton.edu/pub/ubuntu; do
  echo "Checking Ubuntu mirror: $candidate"
  if curl --ipv4 --fail --silent --show-error --location --connect-timeout 5 --max-time 15 \
      "$candidate/dists/jammy/InRelease" --output /dev/null && \
    curl --ipv4 --fail --silent --show-error --location --connect-timeout 5 --max-time 15 \
      "$candidate/dists/jammy-security/InRelease" --output /dev/null; then
    mirror="$candidate"
    break
  fi
done
if [[ -z "$mirror" ]]; then
  echo '::error::No Ubuntu package mirror is reachable. Check runner DNS/network access; no packages were installed.'
  exit 1
fi
echo "Using Ubuntu mirror: $mirror (APT signature verification remains enabled)"

shopt -s nullglob
sources=(/etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources)
for source in "${sources[@]}"; do
  [[ -f "$source" ]] || continue
  sudo sed -i \
    -e "s|mirror+file:/etc/apt/apt-mirrors.txt|$mirror|g" \
    -e "s|https\\?://azure\\.archive\\.ubuntu\\.com/ubuntu|$mirror|g" \
    -e "s|https\\?://archive\\.ubuntu\\.com/ubuntu|$mirror|g" \
    -e "s|https\\?://security\\.ubuntu\\.com/ubuntu|$mirror|g" \
    "$source"
done

apt_options=(-o Acquire::ForceIPv4=true -o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)
# Never proceed using stale indexes after a failed repository update.
sudo apt-get "${apt_options[@]}" -o APT::Update::Error-Mode=any update
sudo apt-get "${apt_options[@]}" install -y --no-install-recommends \
  libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev \
  librsvg2-dev libayatana-appindicator3-dev patchelf libfuse2
