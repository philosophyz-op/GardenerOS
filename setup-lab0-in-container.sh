#!/bin/bash
# 在 openEuler 容器内执行：先进入挂载目录，例如 cd /mnt && bash setup-lab0-in-container.sh
set -euo pipefail

echo "==> 安装基础工具"
dnf install -y curl vim gcc

echo "==> 配置 Rust 镜像环境变量"
if ! grep -q RUSTUP_DIST_SERVER ~/.bashrc 2>/dev/null; then
  cat >> ~/.bashrc << 'EOF'

export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup
EOF
fi
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup

echo "==> 安装 rustup（默认 nightly，与实验一致可再切换）"
if [[ ! -f "$HOME/.cargo/env" ]]; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly
fi
# shellcheck source=/dev/null
source "$HOME/.cargo/env"

echo "==> cargo 使用中科大源"
mkdir -p "$HOME/.cargo"
cat > "$HOME/.cargo/config" << 'EOF'
[source.crates-io]
replace-with = 'ustc'

[source.ustc]
registry = "git://mirrors.ustc.edu.cn/crates.io-index"
EOF

echo "==> 安装 nightly 与实验指定 toolchain"
rustup install nightly
rustup default nightly
rustup install nightly-2022-10-19 || true
rustup default nightly-2022-10-19 2>/dev/null || rustup default nightly

echo "==> Rust 目标与组件"
rustup target add riscv64gc-unknown-none-elf
cargo install cargo-binutils
rustup component add llvm-tools-preview
rustup component add rust-src

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
