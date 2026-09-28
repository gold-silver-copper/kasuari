# sourced by the scripts: paths and builds
HARNESS="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
layouts() { echo "$HARNESS/layouts/$1/target/release/layouts-$1"; }
layouts_bench() { echo "$HARNESS/layouts/$1/target/perf/layouts-$1"; }
solver() { echo "$HARNESS/solver/target/release/solver-harness"; }
build() {
  for v in "$@"; do
    case $v in
      solver) (cd "$HARNESS/solver" && cargo build -q --release 2>&1 | grep -E "^error" -A10) ;;
      *) (cd "$HARNESS/layouts/$v" && cargo build -q --release 2>&1 | grep -E "^error" -A10
          cargo build -q --profile perf 2>&1 | grep -E "^error" -A10) ;;
    esac
  done
  return 0
}
