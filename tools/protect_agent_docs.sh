#!/bin/sh
# Guards the agent instruction files (any CLAUDE.md / AGENTS.md) against changes that arrive from
# the forks (feelx, pp). Operator rule 2026-10-20: these files change only when the operator
# says so; set TI4_AGENT_DOCS_OK=1 for that one commit.
#
# Installed by copying it into the shared hooks directory (`git rev-parse --git-common-dir`/hooks),
# so it covers every worktree and branch, with thin hook wrappers (see install below):
#   pre-commit, pre-merge-commit  -> `protect_agent_docs.sh check`  (refuses the commit)
#   post-merge, post-rewrite      -> `protect_agent_docs.sh restore` (a fast-forward or rebase
#                                    skips the commit hooks: put the files back and say so)
# shortcut: client-side only; a merge done on GitHub is not checked. Upgrade: CODEOWNERS plus
# branch protection on origin.

pattern='(^|/)(claude|agents)\.md$'
mode=${1:-check}

case "$mode" in
install)
    hooks="$(git rev-parse --git-common-dir)/hooks"
    cp -- "$0" "$hooks/protect_agent_docs.sh"
    for h in pre-commit pre-merge-commit; do
        printf '#!/bin/sh\nexec "$(dirname "$0")/protect_agent_docs.sh" check\n' > "$hooks/$h"
    done
    printf '#!/bin/sh\nexec "$(dirname "$0")/protect_agent_docs.sh" restore\n' > "$hooks/post-merge"
    # post-rewrite gets "amend" or "rebase"; only a rebase can bring fork commits in.
    printf '#!/bin/sh\n[ "$1" = rebase ] || exit 0\nexec "$(dirname "$0")/protect_agent_docs.sh" restore\n' \
        > "$hooks/post-rewrite"
    chmod +x "$hooks/protect_agent_docs.sh" "$hooks/pre-commit" "$hooks/pre-merge-commit" \
        "$hooks/post-merge" "$hooks/post-rewrite"
    echo "installed in $hooks"
    ;;
check)
    [ "${TI4_AGENT_DOCS_OK:-}" = 1 ] && exit 0
    changed=$(git diff --cached --name-only --no-renames | grep -i -E "$pattern")
    [ -z "$changed" ] && exit 0
    echo "protect_agent_docs: this commit changes agent instruction files:" >&2
    echo "$changed" | sed 's/^/  /' >&2
    echo "Only the operator changes these. Restore them (git restore --staged --worktree <file>)," >&2
    echo "or, with the operator's say-so, commit with TI4_AGENT_DOCS_OK=1." >&2
    exit 1
    ;;
restore)
    [ "${TI4_AGENT_DOCS_OK:-}" = 1 ] && exit 0
    # post-merge has ORIG_HEAD; post-rewrite (rebase) also leaves ORIG_HEAD at the old tip.
    before=$(git rev-parse -q --verify ORIG_HEAD) || exit 0
    changed=$(git diff --name-only --no-renames "$before" HEAD | grep -i -E "$pattern")
    [ -z "$changed" ] && exit 0
    echo "protect_agent_docs: the merge/rebase changed agent instruction files; putting back the" >&2
    echo "versions from $before in the working tree (commit them to undo the change):" >&2
    echo "$changed" | while read -r f; do
        if git cat-file -e "$before:$f" 2>/dev/null; then
            git restore --source="$before" --worktree --staged -- "$f"
        else
            # New in the merge: leave it in history, take it out of the tree.
            git rm -q -- "$f"
        fi
        echo "  $f" >&2
    done
    exit 0
    ;;
esac
