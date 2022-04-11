#!/bin/bash

set -euxo pipefail

# Compile several crates to gather execution PGO profiles.
# Arg0 => profiles (Debug, Opt)
# Arg1 => scenarios (Full, IncrFull, All)
# Arg2 => crates (syn, cargo, ...)
gather_profiles () {
  cd /checkout/obj

  # Compile libcore, both in opt-level=0 and opt-level=3
  RUSTC_BOOTSTRAP=1 ./build/$PGO_HOST/stage2/bin/rustc \
      --edition=2021 --crate-type=lib ../library/core/src/lib.rs
  RUSTC_BOOTSTRAP=1 ./build/$PGO_HOST/stage2/bin/rustc \
      --edition=2021 --crate-type=lib -Copt-level=3 ../library/core/src/lib.rs

  cd rustc-perf

  # Run rustc-perf benchmarks
  # Benchmark using profile_local with eprintln, which essentially just means
  # don't actually benchmark -- just make sure we run rustc a bunch of times.
  RUST_LOG=collector=debug \
  RUSTC=/checkout/obj/build/$PGO_HOST/stage0/bin/rustc \
  RUSTC_BOOTSTRAP=1 \
  /checkout/obj/build/$PGO_HOST/stage0/bin/cargo run -p collector --bin collector -- \
          profile_local \
          eprintln \
          /checkout/obj/build/$PGO_HOST/stage2/bin/rustc \
          --id Test \
          --profiles $1 \
          --cargo /checkout/obj/build/$PGO_HOST/stage0/bin/cargo \
          --scenarios $2 \
          --include $3

  cd /checkout/obj
}

rm -rf /tmp/rustc-pgo

# We collect LLVM profiling information and rustc profiling information in
# separate phases. This increases build time -- though not by a huge amount --
# but prevents any problems from arising due to different profiling runtimes
# being simultaneously linked in.

python3 ../x.py build --target=$PGO_HOST --host=$PGO_HOST \
    --stage 2 library/std \
    --llvm-profile-generate

# Compile rustc perf
cp -r /tmp/rustc-perf ./
chown -R $(whoami): ./rustc-perf
cd rustc-perf

# Build the collector ahead of time, which is needed to make sure the rustc-fake
# binary used by the collector is present.
RUSTC=/checkout/obj/build/$PGO_HOST/stage0/bin/rustc \
RUSTC_BOOTSTRAP=1 \
/checkout/obj/build/$PGO_HOST/stage0/bin/cargo build -p collector

# Here we're profiling LLVM, so we only care about `Debug` and `Opt`, because we want to stress
# codegen. We also profile some of the most prolific crates.
gather_profiles "Debug,Opt" "Full" \
"syn-1.0.89,cargo-0.60.0,serde-1.0.136,ripgrep-13.0.0,regex-1.5.5,clap-3.1.6,hyper-0.14.18"

# Merge the profile data we gathered for LLVM
# Note that this uses the profdata from the clang we used to build LLVM,
# which likely has a different version than our in-tree clang.
/rustroot/bin/llvm-profdata \
    merge -o /tmp/llvm-pgo.profdata ./build/$PGO_HOST/llvm/build/profiles

# Rustbuild currently doesn't support rebuilding LLVM when PGO options
# change (or any other llvm-related options); so just clear out the relevant
# directories ourselves.
rm -r ./build/$PGO_HOST/llvm ./build/$PGO_HOST/lld

# Okay, LLVM profiling is done, switch to rustc PGO.

python3 ../x.py build --target=$PGO_HOST --host=$PGO_HOST \
    --stage 2 library/std \
    --rust-profile-generate=/tmp/rustc-pgo

# Here we're profiling the `rustc` frontend, so we also include `Check`.
# The benchmark set includes various stress tests that put the frontend under pressure.
gather_profiles "Check,Debug,Opt" "All" \
  "externs,ctfe-stress-4,cargo-0.60.0,token-stream-stress,match-stress,tuple-stress"

# Merge the profile data we gathered
./build/$PGO_HOST/llvm/bin/llvm-profdata \
    merge -o /tmp/rustc-pgo.profdata /tmp/rustc-pgo

# Rustbuild currently doesn't support rebuilding LLVM when PGO options
# change (or any other llvm-related options); so just clear out the relevant
# directories ourselves.
rm -r ./build/$PGO_HOST/llvm ./build/$PGO_HOST/lld

# This produces the actual final set of artifacts, using both the LLVM and rustc
# collected profiling data.
$@ \
    --rust-profile-use=/tmp/rustc-pgo.profdata \
    --llvm-profile-use=/tmp/llvm-pgo.profdata
