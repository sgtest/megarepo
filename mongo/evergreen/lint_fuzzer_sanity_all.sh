DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" > /dev/null 2>&1 && pwd)"
. "$DIR/prelude.sh"

cd src

set -o pipefail
set -o verbose

add_nodejs_to_path

# Run parse-jsfiles on 50 files at a time with 32 processes in parallel.
find "$PWD/jstests" "$PWD/src/mongo/db/modules/enterprise" -name "*.js" -print | xargs -P 32 -L 50 npm run --prefix jstestfuzz parse-jsfiles -- | tee lint_fuzzer_sanity.log
exit_code=$?

activate_venv
$python ./buildscripts/simple_report.py --test-name lint_fuzzer_sanity_all --log-file lint_fuzzer_sanity.log --exit-code $exit_code
exit $exit_code
