#!/bin/bash
# 在 openEuler 容器内执行：先进入挂载目录，例如 cd /mnt && bash setup-lab0-in-container.sh
set -euo pipefail

echo "==> 安装基础工具（含 git：cargo 拉 crates 索引时可走系统 git 并显示进度）"
dnf install -y curl vim gcc git

echo "==> 配置 Rust 镜像环境变量"
if ! grep -q RUSTUP_DIST_SERVER ~/.bashrc 2>/dev/null; then
  cat >> ~/.bashrc << 'EOF'

export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup
EOF
fi
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup

echo "==> 安装 rustup（按实验文档：默认 nightly；多镜像重试，避免 curl 18 部分传输）"
if [[ ! -f "$HOME/.cargo/env" ]]; then
  RUST_ARCH="$(uname -m)-unknown-linux-gnu"
  RUST_INIT="/tmp/rustup-init.$$"
  rustup_try_download() {
    local url="$1"
    echo "  下载: $url"
    # -C - 断点续传; --retry 多次; 避免管道直连 sh 导致无法续传
    curl -fL --connect-timeout 30 --max-time 0 \
      --retry 8 --retry-delay 2 \
      -C - -o "$RUST_INIT" "$url"
  }
  ok=0
  for url in \
    "https://mirrors.ustc.edu.cn/rust-static/rustup/dist/${RUST_ARCH}/rustup-init" \
    "https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup/dist/${RUST_ARCH}/rustup-init" \
    "https://rsproxy.cn/rustup/dist/${RUST_ARCH}/rustup-init" \
    "https://static.rust-lang.org/rustup/dist/${RUST_ARCH}/rustup-init"; do
    rm -f "$RUST_INIT" 2>/dev/null || true
    if rustup_try_download "$url"; then
      chmod +x "$RUST_INIT"
      "$RUST_INIT" -y --default-toolchain nightly
      rm -f "$RUST_INIT"
      ok=1
      break
    fi
    echo "  该地址失败，尝试下一镜像..."
  done
  if [[ "$ok" -ne 1 ]]; then
    echo "错误: 无法从任一镜像下载 rustup-init，请检查容器网络或代理后重试。"
    exit 1
  fi
fi
# shellcheck source=/dev/null
source "$HOME/.cargo/env"

echo "==> cargo 使用国内镜像（HTTPS，避免实验文档里的 git://USTC 在容器内常报 Network is unreachable）"
mkdir -p "$HOME/.cargo"
cat > "$HOME/.cargo/config" << 'EOF'
# 实验原文为 git://USTC；容器内 git:// 常不可达。HTTPS 清华索引首次 clone 体积大，可能 10～30 分钟无新输出，属正常。
[source.crates-io]
replace-with = 'tuna'

[source.tuna]
registry = "https://mirrors.tuna.tsinghua.edu.cn/git/crates.io-index.git"

# 用系统 git 拉索引，终端可见 Counting/receiving 进度，避免误以为卡死
[net]
git-fetch-with-cli = true

# 拉 crate 慢时避免默认过短超时；部分环境 HTTP/2 易断，可缓解 static.crates.io 报错
[http]
timeout = 600
check-revoked = false
EOF

echo "==> 安装 nightly（与实验文档一致；工作目录用 rust-toolchain 固定版本）"
rustup install nightly
rustup default nightly
rustup install nightly-2022-10-19

echo "==> Rust 目标与组件（为 nightly-2022-10-19 一并安装，便于在 /mnt 下构建）"
rustup target add riscv64gc-unknown-none-elf --toolchain nightly-2022-10-19
rustup component add llvm-tools-preview --toolchain nightly-2022-10-19
rustup component add rust-src --toolchain nightly-2022-10-19
# 勿用 0.4.x（edition 2024）。须在 /tmp 下用「最新 nightly」的 cargo 安装：若在 /mnt 会因 rust-toolchain 固定为 1.66；且 1.66 拉 static.crates.io 易 30s 超时/HTTP2 断流。
cd /tmp
rustup run nightly cargo install cargo-binutils --version 0.3.6 --locked

echo "==> 安装 QEMU 5.2 构建依赖（耗时较长）"
dnf groupinstall -y "Development Tools"
dnf install -y autoconf automake gcc gcc-c++ kernel-devel curl libmpc-devel mpfr-devel gmp-devel \
  glib2 glib2-devel make cmake gawk bison flex texinfo gperf libtool patchutils bc \
  python3 ninja-build wget xz

echo "==> 编译安装 QEMU 5.2.0（可能需 10～40 分钟）"
WORKDIR="${TMPDIR:-/tmp}"
cd "$WORKDIR"
if [[ ! -f qemu-5.2.0.tar.xz ]]; then
  wget https://download.qemu.org/qemu-5.2.0.tar.xz
fi
tar xvJf qemu-5.2.0.tar.xz
cd qemu-5.2.0
./configure --target-list=riscv64-softmmu,riscv64-linux-user
make -j"$(nproc)" install

echo "==> 验证"
qemu-system-riscv64 --version
qemu-riscv64 --version
rustc -V

echo "完成。若需保存为镜像：docker ps 查看容器 ID，然后 docker commit ..."
