#!/bin/bash
# usage: setup.sh OLD NEW
#
# Checks out two versions of kasuari as git worktrees for the harness:
#
# - worktrees/old and worktrees/new: the versions to compare (any commit, branch or tag)
# - worktrees/record: NEW, with `add_constraint` printing every constraint to stderr, prefixed
#   with `KASREC` (see scripts/record.sh)
#
# e.g. `./setup.sh v0.4.12 fix/harris-ratio-test`. Rebuild the harness after running it.
set -euo pipefail
cd "$(dirname "$0")"
[ $# = 2 ] || { echo "usage: $0 OLD NEW"; exit 2; }
mkdir -p worktrees
for name in old new record; do
  if [ -d "worktrees/$name" ]; then git worktree remove --force "worktrees/$name"; fi
done
git worktree add --detach worktrees/old "$1" >/dev/null
git worktree add --detach worktrees/new "$2" >/dev/null
git worktree add --detach worktrees/record "$2" >/dev/null

# Cargo can't have two path packages with the same name and version in one lockfile, so old gets
# a placeholder version. It must still match ratatui's requirement (`^0.4.9`), or ratatui would use
# kasuari from crates.io instead of the patched one.
sed -i.bak 's/^version = ".*"/version = "0.4.9999"/' worktrees/old/Cargo.toml
rm worktrees/old/Cargo.toml.bak

python3 - worktrees/record <<'EOF'
import sys
root = sys.argv[1]
lib = open(f"{root}/src/lib.rs").read()
assert "extern crate alloc;" in lib
open(f"{root}/src/lib.rs", "w").write(lib.replace("extern crate alloc;", "extern crate alloc;\nextern crate std;", 1))
solver = open(f"{root}/src/solver.rs").read()
anchor = "    pub fn add_constraint(&mut self, constraint: Constraint) -> Result<(), AddConstraintError> {\n"
assert solver.count(anchor) == 1, "add_constraint not found"
record = anchor + '''        {
            // recording for the harness (see harness/setup.sh)
            let op = match constraint.op() {
                crate::RelationalOperator::Equal => "==",
                crate::RelationalOperator::GreaterOrEqual => ">=",
                crate::RelationalOperator::LessOrEqual => "<=",
            };
            let expression = constraint.expr();
            let mut line = alloc::format!(
                "KASREC {op} {:?} {:?}",
                constraint.strength().value(),
                expression.constant
            );
            for term in &expression.terms {
                let id = alloc::format!("{:?}", term.variable);
                let id = id.trim_start_matches("Variable(").trim_end_matches(')');
                line += &alloc::format!(" {id}:{:?}", term.coefficient);
            }
            std::eprintln!("{line}");
        }
'''
open(f"{root}/src/solver.rs", "w").write(solver.replace(anchor, record, 1))
EOF
echo "old:    $(git -C worktrees/old log --oneline -1)"
echo "new:    $(git -C worktrees/new log --oneline -1)"
echo "record: new, with constraint recording"
