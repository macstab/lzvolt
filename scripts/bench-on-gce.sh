#!/usr/bin/env bash
#
# Run the packer report on rented hardware, once per architecture.
#
#   scripts/bench-on-gce.sh            # Intel, AMD, then ARM
#   scripts/bench-on-gce.sh intel      # just one of them
#   scripts/bench-on-gce.sh amd
#   scripts/bench-on-gce.sh arm
#   KEEP=1 scripts/bench-on-gce.sh     # leave the machines running
#   PROJECT=my-project scripts/bench-on-gce.sh
#   REVS="a1b2c3 d4e5f6" scripts/bench-on-gce.sh intel
#
# REVS measures several revisions on the *same* machine in one visit. Renting a
# machine takes twenty minutes of installing before it measures anything, so
# paying that once for four variants rather than four times is most of what
# makes an optimisation loop bearable -- and it removes the machine as a
# variable between them, which matters more.
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
REVS="${REVS:-}"
KEEP="${KEEP:-0}"
RUNS="${RUNS:-3}"
# Unique per invocation, so two runs at once do not fight over one name --
# which they did, and the loser reported "already exists" from inside the
# zone loop as though the zone were full.
PREFIX="${PREFIX:-keva-bench-$$}"
OUTDIR="${OUTDIR:-bench-results}"

# Compute-optimised on purpose: a shared core gives a number that says more
# about the neighbours than about the code.
#
# Several zones each, because a zone runs out. Compute-optimised machines are
# exactly the ones that do, being the scarce kind, and "try again later" is not
# a plan -- the list is walked until one takes the request. Both types were
# checked to exist in every zone named here, so a failure down the list is a
# stockout and not a typo:
#
#   gcloud compute machine-types list --filter="name=c3-standard-4"
#
# Both start in the same region, which keeps the two machines closer to
# comparable than picking whatever was free on separate continents.
INTEL_ZONES="${INTEL_ZONES:-europe-west4-c europe-west4-b europe-west4-a europe-west1-b europe-west3-a us-central1-a}"
INTEL_TYPE="${INTEL_TYPE:-c3-standard-4}"
INTEL_IMAGE="${INTEL_IMAGE:-debian-12}"
# c3 takes the ordinary balanced disk.
INTEL_DISK="${INTEL_DISK:-pd-balanced}"

AMD_ZONES="${AMD_ZONES:-europe-west4-a europe-west4-b europe-west1-b us-central1-a}"
AMD_TYPE="${AMD_TYPE:-c3d-standard-4}"
AMD_IMAGE="${AMD_IMAGE:-debian-12}"
AMD_DISK="${AMD_DISK:-pd-balanced}"

ARM_ZONES="${ARM_ZONES:-europe-west4-c europe-west4-b europe-west4-a europe-west1-b europe-west3-a us-central1-a}"
ARM_TYPE="${ARM_TYPE:-c4a-standard-4}"
ARM_IMAGE="${ARM_IMAGE:-debian-12-arm64}"
# Axion refuses pd-balanced outright; hyperdisk is the only balanced type it
# takes, and the refusal arrives from the create call rather than from a check.
ARM_DISK="${ARM_DISK:-hyperdisk-balanced}"

case "${1:-both}" in
    intel|amd|arm|both) ;;
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
TMPERR="$(mktemp -t keva-err-XXXXXX)"
trap 'rm -f "$ARCHIVE"* "$RUNNER" "$TMPERR"' EXIT

if [ -z "$REVS" ]; then
    REVS="$BRANCH"
fi
SHORT=""
for rev in $REVS; do
    git rev-parse --verify "$rev" >/dev/null 2>&1 || {
        echo "no such revision: $rev" >&2; exit 1; }
    sha="$(git rev-parse --short "$rev")"
    git archive --format=tar.gz --prefix=keva/ -o "$ARCHIVE.$sha" "$rev"
    SHORT="$SHORT $sha"
    echo "== packing $rev at $sha, $(du -h "$ARCHIVE.$sha" | cut -f1)"
done
SHORT="${SHORT# }"

# What runs on the far side. Quoted heredoc: nothing here is expanded locally.
cat > "$RUNNER" <<'REMOTE'
set -uo pipefail
runs="${1:-3}"
export KEVA_COMMIT="${2:-unknown}"
report="$HOME/report-$KEVA_COMMIT.txt"

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
sudo apt-get install -y -qq build-essential liblz4-dev pkg-config python3 \
    "linux-perf-$(uname -r | cut -d- -f1-2)" linux-perf >/dev/null 2>&1 || \
    sudo apt-get install -y -qq build-essential liblz4-dev pkg-config python3 >/dev/null

if ! command -v cargo >/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y -q
fi
. "$HOME/.cargo/env"
echo "rustc     $(rustc --version)"
echo

rm -rf "$HOME/keva"
tar xzf "$HOME/keva-$KEVA_COMMIT.tar.gz" -C "$HOME"
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

# What the machine says, rather than what the code looks like.
#
# Every micro-optimisation this evening was reasoned from a model of the part
# and measured afterwards, and the model was wrong about as often as it was
# right -- indexed addressing, micro-op width, branch density. Counters are the
# thing that would have said so in advance. They are informational: a shared
# runner cannot support a threshold, and the point is the ratios between
# counters within one run.
echo
echo "== counters, decoding records_512 and records_64k"
if command -v perf >/dev/null && perf stat true >/dev/null 2>&1; then
    for shape in records_512 records_64k; do
        echo "-- $shape"
        perf stat -e cycles,instructions,branches,branch-misses,\
cache-references,cache-misses,ld_blocks.store_forward,stalled-cycles-frontend \
            -x, --no-big-num \
            cargo run -q --release -p keva-core --features liblz4 \
                --example counters -- "$shape" own 2>&1 |
            grep -E '^[0-9]' | awk -F, '{printf "   %-28s %s\n", $3, $1}'
    done
else
    echo "   perf is not available on this instance"
fi

echo
echo "== throughput, $runs runs"
for i in $(seq 1 "$runs"); do
    echo
    echo "---------- run $i ----------"
    scripts/packer-report.sh "$HOME/run-$KEVA_COMMIT-$i.txt" || echo "   report failed"
done
REMOTE

one () {
    local label="$1" zones="$2" mtype="$3" image="$4" disk="$5"
    local vm="$PREFIX-$label"
    zone=""                                   # global on purpose: the EXIT trap
                                              # reads it after this returns

    echo
    echo "=============================================================="
    echo "== $label: $mtype, $disk"
    echo "=============================================================="

    local keep="$KEEP"
    cleanup () {
        [ -n "${zone:-}" ] || return 0
        if [ "$keep" = "1" ]; then
            echo "== keeping $vm (KEEP=1)"
        else
            echo "== deleting $vm"
            "${GC[@]}" compute instances delete "$vm" --zone="$zone" --quiet >/dev/null 2>&1 || true
        fi
    }
    trap 'cleanup; rm -f "$ARCHIVE" "$RUNNER" "$TMPERR"' EXIT

    # A zone that is out of this machine type says so and names the ones that
    # are not. Walking the list is simpler than parsing that, and it also covers
    # a zone that does not offer the type at all.
    local z created=0
    for z in $zones; do
        echo "== creating in $z"
        if "${GC[@]}" compute instances create "$vm" \
            --zone="$z" --machine-type="$mtype" \
            --image-family="$image" --image-project=debian-cloud \
            --boot-disk-size=50GB --boot-disk-type="$disk" \
            --quiet >/dev/null 2>"$TMPERR"; then
            zone="$z"
            created=1
            break
        fi
        if grep -q 'ZONE_RESOURCE_POOL_EXHAUSTED\|does not have enough resources\|not available in zone' "$TMPERR"; then
            echo "   out of capacity, next zone"
        else
            sed 's/^/   /' "$TMPERR" >&2
            break
        fi
    done
    if [ "$created" -ne 1 ]; then
        echo "== could not create $vm in any of: $zones" >&2
        return 1
    fi

    echo "== waiting for ssh"
    local tries=0
    until "${GC[@]}" compute ssh "$vm" --zone="$zone" --quiet --command=true >/dev/null 2>&1; do
        tries=$((tries + 1))
        [ "$tries" -lt 40 ] || { echo "ssh never came up" >&2; return 1; }
        sleep 5
    done

    echo "== uploading"
    for sha in $SHORT; do
        "${GC[@]}" compute scp "$ARCHIVE.$sha" "$vm:~/keva-$sha.tar.gz" \
            --zone="$zone" --quiet >/dev/null
    done
    "${GC[@]}" compute scp "$RUNNER" "$vm:~/run.sh" --zone="$zone" --quiet >/dev/null

    for sha in $SHORT; do
        echo "== running $sha (apt and rustc are paid once, on the first)"
        "${GC[@]}" compute ssh "$vm" --zone="$zone" --quiet \
            --command="bash ~/run.sh $RUNS $sha" || echo "== $sha reported a failure"
    done

    echo "== downloading"
    "${GC[@]}" compute scp "$vm:~/report-*.txt" "$OUTDIR/" --zone="$zone" --quiet >/dev/null 2>&1 || true
    "${GC[@]}" compute scp "$vm:~/run-*.txt" "$OUTDIR/" --zone="$zone" --quiet >/dev/null 2>&1 || true
    for f in "$OUTDIR"/report-*.txt "$OUTDIR"/run-*.txt; do
        [ -e "$f" ] || continue
        case "$(basename "$f")" in
            "$label"-*) continue ;;
        esac
        mv "$f" "$OUTDIR/$label-$(basename "$f")"
    done

    cleanup
    trap 'rm -f "$ARCHIVE" "$RUNNER" "$TMPERR"' EXIT
}

case "${1:-both}" in
    intel) one intel "$INTEL_ZONES" "$INTEL_TYPE" "$INTEL_IMAGE" "$INTEL_DISK" ;;
    amd)   one amd   "$AMD_ZONES"   "$AMD_TYPE"   "$AMD_IMAGE"   "$AMD_DISK" ;;
    arm)   one arm   "$ARM_ZONES"   "$ARM_TYPE"   "$ARM_IMAGE"   "$ARM_DISK" ;;
    both)
        # One failing must not take the other with it: the point of two
        # machines is two independent data points.
        one intel "$INTEL_ZONES" "$INTEL_TYPE" "$INTEL_IMAGE" "$INTEL_DISK" || true
        one amd   "$AMD_ZONES"   "$AMD_TYPE"   "$AMD_IMAGE"   "$AMD_DISK" || true
        one arm   "$ARM_ZONES"   "$ARM_TYPE"   "$ARM_IMAGE"   "$ARM_DISK" || true
        ;;
esac

# Anything still standing from an earlier crash, whatever it was called.
stragglers="$("${GC[@]}" compute instances list --filter='name~^keva-bench' \
    --format='value(name,zone)' 2>/dev/null || true)"
if [ -n "$stragglers" ]; then
    echo
    echo "== instances still up from an earlier run:"
    echo "$stragglers" | sed 's/^/   /'
    echo "   delete them with: gcloud --project=$PROJECT compute instances delete NAME --zone=ZONE"
fi

echo
echo "== results in $OUTDIR"
ls -1 "$OUTDIR" | sed 's/^/   /'
echo
echo "Only ratios taken inside one run mean anything; the repeated runs are"
echo "there to show the spread, not to be averaged."
