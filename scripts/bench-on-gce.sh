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
#   QUICK=1 REVS="..." scripts/bench-on-gce.sh intel   # direction only, ~2 min/rev
#   KEVA_FILTER='own_format' ...                       # narrow what is measured
#
# REVS measures several revisions on the *same* machine in one visit. Renting a
# machine takes twenty minutes of installing before it measures anything, so
# paying that once for four variants rather than four times is most of what
# makes an optimisation loop bearable -- and it removes the machine as a
# variable between them, which matters more.
#
# It introduces one of its own, and on AMD it is large. Measured with two
# byte-identical revisions -- verified identical in the text section, not
# assumed -- whichever runs *second* on a c3d is about 11% slower on the
# 512-byte decoder cells:
#
#   position 1   fd6c4ba 9.495   aa9d7b3 9.343
#   position 2   aa9d7b3 8.478   fd6c4ba 8.449
#
# Reversing the order moves the penalty with the position, not with the code.
# It is not a uniform clock effect: the 4 KiB and 64 KiB cells lose 1-3% while
# the 512-byte ones lose 11%.
#
# So on AMD, compare position against position -- run the pair in both orders
# and match first against first -- or give each revision its own machine. A
# straight REVS A B reading on a c3d will report an 11% regression for a change
# that is not there, and an 11% win for one that is not either. Intel and ARM do
# not show it at this size, which is not evidence that they never will.
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
# A machine costs twenty minutes before it measures anything: apt, rustup, the
# first build. That is the whole of the wait in a development loop, and it
# should be paid once rather than per question.
#
#   KEEP=1  leaves the machine running when the run ends
#   REUSE=1 attaches to a machine that is already running instead of creating
#           one, and leaves it running afterwards
#
# Together they turn "does this change help" from half an hour into two
# minutes. Remember to run without them, or `scripts/bench-on-gce.sh --drop`,
# when the loop is over: a kept machine is a bill.
REUSE="${REUSE:-0}"
[ "$REUSE" = 1 ] && KEEP=1
RUNS="${RUNS:-3}"
# Direction-finding rather than publication. QUICK=1 runs one check instead of
# four and four benchmark cells instead of eighteen, which turns twelve minutes
# a revision into about two. The long form is for the numbers that get quoted;
# using it to decide whether a change helps is most of an evening.
QUICK="${QUICK:-0}"
# Two seconds is enough to see a five-percent move on most cells. records_512 is
# not most cells: it has returned 6.02 and 7.53 for the same code in one
# evening, so a verdict there needs TIME=5 and a revision in front of it to
# absorb the cold start a fresh machine gives whatever is measured first.
TIME="${TIME:-1}"
# Which benchmark groups the quick run measures. Packing costs ten times what
# decoding does, so a decoder question that leaves compress3 in spends two
# thirds of its wall clock producing numbers it will not read.
# Not GROUPS: bash defines that one itself, holding the caller's group ids, and
# assigning to it is silently ignored. So the filter reached the remote runner as
# "20" and Criterion matched that string against benchmark ids -- which selects
# `pack/sizes/pack/2048` and nothing else. Three machines were rented to measure
# two cells nobody asked for. See docs/MEASUREMENTS.md.
KEVA_FILTER="${KEVA_FILTER:-compress3|own_format|same_bytes}"
WARM="${WARM:-1}"
# Unique per invocation, so two runs at once do not fight over one name --
# which they did, and the loser reported "already exists" from inside the
# zone loop as though the zone were full.
# Unique per invocation by default, so two runs at once do not fight over one
# name. REUSE needs a name that survives the shell, so it gets a fixed one.
PREFIX_BASE="${PREFIX_BASE:-keva-bench}"
if [ "${REUSE:-0}" = 1 ]; then
    PREFIX="${PREFIX:-$PREFIX_BASE}"
else
    PREFIX="${PREFIX:-$PREFIX_BASE-$$}"
    PREFIX_BASE="$PREFIX"
fi
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
export KEVA_QUICK="${3:-0}"
export KEVA_TIME="${4:-1}"
export KEVA_WARM="${5:-1}"
export KEVA_GROUPS="${6:-compress3|own_format|same_bytes}"
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
sudo apt-get install -y -qq build-essential liblz4-dev pkg-config python3 >/dev/null
sudo apt-get install -y -qq linux-perf >/dev/null 2>&1 || true
# Counters are readable by an unprivileged process only below 2.
sudo sysctl -q -w kernel.perf_event_paranoid=1 >/dev/null 2>&1 || true

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
# In quick mode only the check that can catch a broken kernel runs: the
# differential and interop tests, with the reference library linked. The other
# three are full rebuilds in other configurations and they cost more than the
# measurement does. They belong to the run that produces a published number,
# not to the run that answers "which direction".
if [ "${KEVA_QUICK:-0}" = 1 ]; then
    check -p keva-core --features liblz4
else
    check --workspace
    check --workspace --release
    check --workspace --no-default-features
    check -p keva-core --features liblz4
fi

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
    echo "   no counters here:"
    echo "     perf binary: $(command -v perf || echo absent)"
    echo "     PMUs: $(ls /sys/bus/event_source/devices/ 2>/dev/null | tr '\n' ' ')"
    echo "   a 'cpu' PMU appears only when the instance was created with"
    echo "   --performance-monitoring-unit, which this script now asks for."
fi

echo
if [ "${KEVA_QUICK:-0}" = 1 ]; then
    # Direction, not publication. Four shapes rather than nine, one run rather
    # than three, and two seconds of measurement rather than the default five --
    # enough to see a five-percent move, which is the size worth acting on.
    #
    # The shapes are the two the work is aimed at and the two that have to not
    # get worse while it happens.
    echo "== quick: every shape, one second a cell"
    # All sixty cells -- nine shapes packing, six and five decoding, three
    # implementations each -- at one second of measurement instead of five.
    #
    # The time was never in the shapes, it was in Criterion's default: three
    # seconds of warm-up and five of measurement per cell, about nine seconds
    # sixty times over. At one second it is two minutes for the whole table.
    # Cutting shapes therefore saved nothing and only created blind spots --
    # two changes were called flat this evening against cells they could not
    # touch.
    #
    # This answers "did it move", not "by exactly how much". A cell that swings
    # five percent between revisions is worth another look at five seconds; one
    # that does not is decided.
    # KEVA_FILTER=... narrows what is measured. Packing is ten times the cost of
    # decoding, so a decoder question that measures compress3 as well spends
    # two thirds of its wall clock on numbers it will not read.
    cargo bench -q -p keva-core --features liblz4 --bench pack -- \
        --measurement-time ${KEVA_TIME:-2} --warm-up-time ${KEVA_WARM:-1} --noplot \
        "${KEVA_GROUPS:-compress3|own_format|same_bytes}" \
        2>&1 | tee "$HOME/run-$KEVA_COMMIT-1.txt" |
        grep -E 'compress3|own_format|same_bytes|thrpt' || echo "   quick bench failed"
else
    echo "== throughput, $runs runs"
    for i in $(seq 1 "$runs"); do
        echo
        echo "---------- run $i ----------"
        scripts/packer-report.sh "$HOME/run-$KEVA_COMMIT-$i.txt" || echo "   report failed"
    done
fi
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
    # The performance counters are off unless the instance is created asking for
    # them: a stock GCE guest sees no `cpu` under /sys/bus/event_source/devices,
    # so perf can report software events and nothing else. Found by installing
    # perf and getting counters anyway -- the machine had never been asked.
    #
    # Not every machine type accepts the request. c3d refuses it outright on API
    # v1, and a run that cannot have counters is still worth far more than no run
    # at all, so the flag is dropped and the attempt repeated rather than treated
    # as a failure. The report says which of the two happened, because a table
    # without counters should not look like one that had them and found nothing.
    local z created=0

    if [ "$REUSE" = 1 ]; then
        local found
        found="$("${GC[@]}" compute instances list \
            --filter="name~'^$PREFIX_BASE-$label$' AND status=RUNNING" \
            --format='value(name,zone)' 2>/dev/null | head -1)"
        if [ -n "$found" ]; then
            vm="${found%%[[:space:]]*}"
            zone="${found##*[[:space:]]}"
            created=1
            echo "== reusing $vm in $zone"
        else
            echo "== REUSE=1 but no $PREFIX_BASE-$label is running; creating one"
        fi
    fi

    [ "$created" = 1 ] || for z in $zones; do
        echo "== creating in $z"
        if "${GC[@]}" compute instances create "$vm" \
            --zone="$z" --machine-type="$mtype" \
            --image-family="$image" --image-project=debian-cloud \
            --boot-disk-size=50GB --boot-disk-type="$disk" \
            --performance-monitoring-unit=standard \
            --quiet >/dev/null 2>"$TMPERR"; then
            zone="$z"
            created=1
            break
        fi
        if grep -q 'performanceMonitoringUnit\|PerformanceMonitoringUnit' "$TMPERR"; then
            echo "   $mtype will not take a PMU here, retrying without counters"
            if "${GC[@]}" compute instances create "$vm" \
                --zone="$z" --machine-type="$mtype" \
                --image-family="$image" --image-project=debian-cloud \
                --boot-disk-size=50GB --boot-disk-type="$disk" \
                --quiet >/dev/null 2>"$TMPERR"; then
                zone="$z"
                created=1
                break
            fi
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

    # Detached, and polled for.
    #
    # This used to hold one SSH session open for the whole benchmark, which
    # meant the run lived exactly as long as the connection did. On 2026-09-24
    # three machines lost their second revision to a "connection reset by peer"
    # at the same line of all three logs -- an hour of rented time each, and
    # nothing to show, because the remote process went down with the session.
    #
    # Now the runner is launched with nohup and the local side asks every thirty
    # seconds whether the run is done. A reset costs one poll.
    #
    # The marker is written by the wrapper after the runner returns, and not by
    # the runner itself. The first cut polled for `report-$sha.txt`, which the
    # runner creates when it *starts*: the poll was satisfied after thirty
    # seconds, both revisions were launched on top of each other, and the machine
    # was deleted out from under them. Three more machines, no measurement, and
    # the log said "0 failures" the whole way.
    for sha in $SHORT; do
        echo "== running $sha (apt and rustc are paid once, on the first)"
        inner="bash ~/run.sh $RUNS $sha $QUICK $TIME $WARM '$KEVA_FILTER' > ~/log-$sha.txt 2>&1; echo done > ~/done-$sha"
        "${GC[@]}" compute ssh "$vm" --zone="$zone" --quiet \
            --command="rm -f ~/done-$sha ~/log-$sha.txt; nohup bash -c \"$inner\" >/dev/null 2>&1 </dev/null & echo launched" \
            || echo "== $sha did not launch"
        ok=""
        for _ in $(seq 1 160); do
            sleep 30
            if "${GC[@]}" compute ssh "$vm" --zone="$zone" --quiet \
                --command="test -f ~/done-$sha" >/dev/null 2>&1; then ok=1; break; fi
        done
        "${GC[@]}" compute ssh "$vm" --zone="$zone" --quiet \
            --command="cat ~/log-$sha.txt" 2>/dev/null || true
        [ -n "$ok" ] || echo "== $sha reported a failure"
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
