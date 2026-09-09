#!/usr/bin/env bash
#
# Run the packer report on rented hardware, once per architecture.
#
#   scripts/bench-on-gce.sh            # Intel, then ARM
#   scripts/bench-on-gce.sh intel      # just one of them
#   scripts/bench-on-gce.sh arm
#   KEEP=1 scripts/bench-on-gce.sh     # leave the machines running
#   PROJECT=my-project scripts/bench-on-gce.sh
#
# The project is named on every call rather than taken from whatever gcloud
# happens to have configured. A script that creates instances in an unstated
# project is a way to bill the wrong one.
#
# Everything this project claims about the packer was measured on one laptop,
# on one microarchitecture. That is a limit on what the numbers are evidence
# for, and it is a cheap one to lift: two machines, a quarter of an hour, small
# change. The Intel run is the first time the SSE2 decoder executes on hardware
# rather than under a translator; the ARM run is the first time the AArch64 one
# executes on anything that is not an Apple part.
#
# Correctness is checked before throughput and its result goes in the report.
# A test that fails here is the most valuable thing this script can produce, so
# it is recorded rather than allowed to abort the run.
#
# The machines are deleted on the way out, including when something fails --
# a benchmark script that leaves instances running is a bill, not a tool. Set
# KEEP=1 to keep them for debugging.

set -euo pipefail

cd "$(dirname "$0")/.."

BRANCH="${BRANCH:-packer}"
KEEP="${KEEP:-0}"
RUNS="${RUNS:-3}"
PREFIX="${PREFIX:-keva-bench}"
OUTDIR="${OUTDIR:-bench-results}"

# Compute-optimised on purpose: a shared core gives a number that says more
# about the neighbours than about the code.
INTEL_ZONE="${INTEL_ZONE:-europe-west4-a}"
INTEL_TYPE="${INTEL_TYPE:-c3-standard-4}"
INTEL_IMAGE="${INTEL_IMAGE:-debian-12}"

ARM_ZONE="${ARM_ZONE:-us-central1-a}"
ARM_TYPE="${ARM_TYPE:-c4a-standard-4}"
ARM_IMAGE="${ARM_IMAGE:-debian-12-arm64}"

case "${1:-both}" in
    intel|arm|both) ;;
    *) echo "usage: $0 [intel|arm|both]" >&2; exit 1 ;;
esac

command -v gcloud >/dev/null || { echo "gcloud is not installed" >&2; exit 1; }

PROJECT="${PROJECT:-$(gcloud config get-value project 2>/dev/null)}"
if [ -z "$PROJECT" ] || [ "$PROJECT" = "(unset)" ]; then
    echo "no project. Set one:" >&2
    echo "    PROJECT=<id> $0 ${1:-}" >&2
    echo "  or  gcloud config set project <id>" >&2
    echo >&2
    gcloud projects list --format='table(projectId,name)' >&2 || true
    exit 1
fi
# A project *number* is configured often enough to be worth catching here:
# gcloud takes it in some places and refuses it in others, and the refusal
# arrives halfway through creating an instance.
case "$PROJECT" in
    ''|*[!0-9]*) ;;
    *)
        resolved="$(gcloud projects describe "$PROJECT" --format='value(projectId)' 2>/dev/null || true)"
        if [ -n "$resolved" ]; then
            echo "== $PROJECT is a project number; using $resolved"
            PROJECT="$resolved"
        fi
        ;;
esac
GC=(gcloud --project="$PROJECT")

echo "== project $PROJECT"
"${GC[@]}" compute zones list --limit=1 --format='value(name)' >/dev/null 2>&1 || {
    echo "cannot reach Compute Engine in $PROJECT -- is the API enabled?" >&2
    echo "    gcloud services enable compute.googleapis.com --project=$PROJECT" >&2
    exit 1
}
git rev-parse --verify "$BRANCH" >/dev/null 2>&1 || {
    echo "no such branch: $BRANCH" >&2; exit 1; }

mkdir -p "$OUTDIR"

ARCHIVE="$(mktemp -t keva-packer-XXXXXX).tar.gz"
RUNNER="$(mktemp -t keva-runner-XXXXXX).sh"
trap 'rm -f "$ARCHIVE" "$RUNNER"' EXIT

git archive --format=tar.gz --prefix=keva/ -o "$ARCHIVE" "$BRANCH"
COMMIT="$(git rev-parse --short "$BRANCH")"
echo "== packing $BRANCH at $COMMIT, $(du -h "$ARCHIVE" | cut -f1)"

# What runs on the far side. Quoted heredoc: nothing here is expanded locally.
cat > "$RUNNER" <<'REMOTE'
set -uo pipefail
runs="${1:-3}"
report="$HOME/report.txt"

exec > >(tee "$report") 2>&1

echo "# packer on rented hardware"
echo
echo "date      $(date -u '+%Y-%m-%d %H:%M:%SZ')"
echo "host      $(uname -srm)"
echo "cpu       $(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2- | sed 's/^ *//' || echo unknown)"
echo "cores     $(nproc)"
echo

export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -qq
sudo apt-get install -y -qq build-essential liblz4-dev pkg-config python3 >/dev/null

if ! command -v cargo >/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y -q
fi
. "$HOME/.cargo/env"
echo "rustc     $(rustc --version)"
echo

tar xzf "$HOME/keva-packer.tar.gz" -C "$HOME"
cd "$HOME/keva"

# Correctness first, and its result is part of the report. The kernels are
# per-architecture, so this is the run that decides whether the numbers below
# describe working code.
echo "== tests"
fail=0
check () {
    local out status
    out="$(cargo test "$@" 2>&1)"
    status=$?
    echo "$out" | grep -E '^test result|^error' || true
    echo "   [$*] -> exit $status"
    [ "$status" -eq 0 ] || fail=1
}
check --workspace
check --workspace --release
check --workspace --no-default-features
check -p keva-core --features liblz4

if [ "$fail" -ne 0 ]; then
    echo
    echo "!! a test failed on this machine -- the numbers below describe code"
    echo "!! that does not pass its own suite here. That is the finding."
    echo
fi

echo
echo "== throughput, $runs runs"
for i in $(seq 1 "$runs"); do
    echo
    echo "---------- run $i ----------"
    scripts/packer-report.sh "$HOME/run-$i.txt" || echo "   report failed"
done
REMOTE

one () {
    local label="$1" zone="$2" mtype="$3" image="$4"
    local vm="$PREFIX-$label"

    echo
    echo "=============================================================="
    echo "== $label: $mtype in $zone"
    echo "=============================================================="

    local keep="$KEEP"
    cleanup () {
        if [ "$keep" = "1" ]; then
            echo "== keeping $vm (KEEP=1)"
        else
            echo "== deleting $vm"
            "${GC[@]}" compute instances delete "$vm" --zone="$zone" --quiet >/dev/null 2>&1 || true
        fi
    }
    trap 'cleanup; rm -f "$ARCHIVE" "$RUNNER"' EXIT

    "${GC[@]}" compute instances create "$vm" \
        --zone="$zone" --machine-type="$mtype" \
        --image-family="$image" --image-project=debian-cloud \
        --boot-disk-size=50GB --boot-disk-type=pd-balanced \
        --quiet >/dev/null

    echo "== waiting for ssh"
    local tries=0
    until "${GC[@]}" compute ssh "$vm" --zone="$zone" --quiet --command=true >/dev/null 2>&1; do
        tries=$((tries + 1))
        [ "$tries" -lt 40 ] || { echo "ssh never came up" >&2; return 1; }
        sleep 5
    done

    echo "== uploading"
    "${GC[@]}" compute scp "$ARCHIVE" "$vm:~/keva-packer.tar.gz" --zone="$zone" --quiet >/dev/null
    "${GC[@]}" compute scp "$RUNNER" "$vm:~/run.sh" --zone="$zone" --quiet >/dev/null

    echo "== running (this takes a while: apt, rustc, then the benchmarks)"
    "${GC[@]}" compute ssh "$vm" --zone="$zone" --quiet \
        --command="bash ~/run.sh $RUNS" || echo "== the remote run reported a failure"

    echo "== downloading"
    "${GC[@]}" compute scp "$vm:~/report.txt" "$OUTDIR/report-$label-$COMMIT.txt" \
        --zone="$zone" --quiet >/dev/null || echo "== no report came back"
    "${GC[@]}" compute scp "$vm:~/run-*.txt" "$OUTDIR/" --zone="$zone" --quiet >/dev/null 2>&1 || true
    for f in "$OUTDIR"/run-*.txt; do
        [ -e "$f" ] || continue
        mv "$f" "$OUTDIR/$label-$COMMIT-$(basename "$f")"
    done

    cleanup
    trap 'rm -f "$ARCHIVE" "$RUNNER"' EXIT
}

case "${1:-both}" in
    intel) one intel "$INTEL_ZONE" "$INTEL_TYPE" "$INTEL_IMAGE" ;;
    arm)   one arm   "$ARM_ZONE"   "$ARM_TYPE"   "$ARM_IMAGE" ;;
    both)
        one intel "$INTEL_ZONE" "$INTEL_TYPE" "$INTEL_IMAGE"
        one arm   "$ARM_ZONE"   "$ARM_TYPE"   "$ARM_IMAGE"
        ;;
    *) echo "usage: $0 [intel|arm|both]" >&2; exit 1 ;;
esac

echo
echo "== results in $OUTDIR"
ls -1 "$OUTDIR" | sed 's/^/   /'
echo
echo "Only ratios taken inside one run mean anything; the repeated runs are"
echo "there to show the spread, not to be averaged."
