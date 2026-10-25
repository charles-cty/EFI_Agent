"""Actual BusyBox ash Bash extension checks across fresh task boundaries."""

BASH_COMPAT_CHECK = r'''
compat_value='one two one'
test "$compat_value" = 'one two one' || exit 91
test "${compat_value//one/X}" = 'X two X' || exit 92
test "${compat_value:4:3}" = two || exit 93
test "$(printf '%s' $'a\tb\n\'\\')" = "$(printf 'a\tb\n\047\\')" || exit 94
function compat_function() { [[ "$1" == prefix* ]] && [[ "$1" =~ ^prefix[0-9]+$ ]]; }
compat_function prefix731 || exit 95
if compat_function prefixBAD; then exit 96; fi
test "$(compat_function prefix732 && echo nested)" = nested || exit 97
test "$(printf x | (compat_function prefix733 && cat))" = x || exit 98
set -o pipefail
(exit 7) | true
test "$?" = 7 || exit 99
(exit 7) | (exit 9) | true
test "$?" = 9 || exit 100
test "$( (exit 3) | true; echo "$?" )" = 3 || exit 101
set +o pipefail
(exit 7) | true
test "$?" = 0 || exit 102
printf 'compat_sourced=731\n' > /tmp/compat-source
source /tmp/compat-source
test "$compat_sourced" = 731 || exit 103
{ printf out; printf err >&2; } &> /tmp/compat-redirection
{ printf more; printf errors >&2; } >> /tmp/compat-redirection 2>&1
test "$(cat /tmp/compat-redirection)" = outerrmoreerrors || exit 104
printf x | { cat; printf err >&2; } &> /tmp/compat-task-redirection
test "$(cat /tmp/compat-task-redirection)" = xerr || exit 112
exec 8> /tmp/compat-trace
BASH_XTRACEFD=8
set -x
compat_trace=731
(compat_trace=732)
set +x
unset BASH_XTRACEFD
grep -q 'compat_trace=731' /tmp/compat-trace || exit 105
grep -q 'compat_trace=732' /tmp/compat-trace || exit 106
test "$EPOCHSECONDS" -gt 1700000000 || exit 107
case "$EPOCHREALTIME" in *.*) :;; *) exit 108;; esac
test "$SHLVL" -ge 1 || exit 109
sleep 0.01 &
compat_wait=$!
wait -n
test "$?" = 0 || exit 110
(sleep 0.01; exit 7) &
compat_wait=$!
wait "$compat_wait"
test "$?" = 7 || exit 111
echo ASH_BASH_COMPAT_VERIFIED
'''
