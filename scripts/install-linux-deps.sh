#!/usr/bin/env bash
set -euo pipefail

# Keep the Ubuntu 22.04 compatibility baseline, without the hosted runner's
# Azure mirror list, which can stall on DNS failures for every package.
shopt -s nullglob
sources=(/etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources)
for source in "${sources[@]}"; do
  [[ -f "$source" ]] || continue
  sudo sed -i \
    -e 's|mirror+file:/etc/apt/apt-mirrors.txt|https://archive.ubuntu.com/ubuntu|g' \
    -e 's|https\?://azure.archive.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
    -e 's|http://archive.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
    -e 's|http://security.ubuntu.com/ubuntu|https://security.ubuntu.com/ubuntu|g' \
    "$source"
done

timeout 15s getent ahostsv4 archive.ubuntu.com > /dev/null
apt_options=(-o Acquire::ForceIPv4=true -o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)
# Never proceed using stale indexes after a failed repository update.
sudo apt-get "${apt_options[@]}" -o APT::Update::Error-Mode=any update
sudo apt-get "${apt_options[@]}" install -y --no-install-recommends \
  libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev \
  librsvg2-dev libayatana-appindicator3-dev patchelf libfuse2
