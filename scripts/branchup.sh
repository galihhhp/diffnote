#!/bin/sh
set -u

target=""
base="main"
exists=false
pop=true
pull_rebase=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --exists|-e)
      exists=true
      shift
      ;;
    --no-pop)
      pop=false
      shift
      ;;
    --base)
      base="$2"
      shift 2
      ;;
    --rebase)
      pull_rebase=true
      shift
      ;;
    --merge)
      pull_rebase=false
      shift
      ;;
    *)
      target="$1"
      shift
      ;;
  esac
done

if [ -z "$target" ]; then
  echo "Usage: branchup <branch-name> [--base <branch>] [--rebase] [--exists] [--no-pop]"
  exit 1
fi

current=$(git symbolic-ref --short HEAD 2>/dev/null || true)
if [ "$current" = "$target" ]; then
  echo "Error: Already on branch '$target'"
  exit 1
fi

if git rev-parse --verify REBASE_HEAD >/dev/null 2>&1 || [ -d .git/rebase-merge ] || [ -d .git/rebase-apply ]; then
  echo "Error: Rebase in progress. Finish or abort it first"
  exit 1
fi

if [ -f .git/MERGE_HEAD ]; then
  echo "Error: Merge in progress. Finish or abort it first"
  exit 1
fi

if ! git symbolic-ref HEAD >/dev/null 2>&1; then
  echo "Error: Detached HEAD. Switch to a branch first"
  exit 1
fi

if ! git show-ref --verify --quiet "refs/heads/$base"; then
  echo "Error: Base branch '$base' doesn't exist locally"
  exit 1
fi

if [ "$exists" = true ]; then
  if ! git show-ref --verify --quiet "refs/heads/$target"; then
    echo "Error: --exists passed but branch '$target' doesn't exist"
    exit 1
  fi
else
  if git show-ref --verify --quiet "refs/heads/$target"; then
    echo "Error: Branch '$target' already exists. Use --exists to switch to it"
    exit 1
  fi
fi

did_stash=false
stash_count_before=$(git stash list 2>/dev/null | wc -l | tr -d ' ')

if [ -n "$(git status --porcelain)" ]; then
  echo "Stashing uncommitted changes..."
  git stash push -u -m "auto-stash before branchup to $target" || exit 1
  did_stash=true
fi

stash_count_after=$(git stash list 2>/dev/null | wc -l | tr -d ' ')

if [ "$did_stash" = true ] && [ "$stash_count_after" -le "$stash_count_before" ]; then
  echo "Error: Stash created but not detected. Aborting to avoid popping old stash"
  git stash pop 2>/dev/null
  exit 1
fi

echo "Pulling latest '$base'..."
if ! git checkout "$base"; then
  if [ "$did_stash" = true ]; then git stash pop 2>/dev/null; fi
  exit 1
fi

if [ "$pull_rebase" = true ]; then
  echo "Pulling with rebase..."
  git pull --rebase origin "$base" || exit 1
else
  echo "Pulling with merge..."
  git pull origin "$base" || exit 1
fi

if [ "$exists" = true ]; then
  echo "Switching to existing branch '$target'..."
  if ! git checkout "$target"; then
    git checkout "$base"
    if [ "$did_stash" = true ]; then git stash pop 2>/dev/null; fi
    exit 1
  fi
else
  echo "Creating '$target' from latest '$base'..."
  if ! git checkout -b "$target"; then
    git checkout "$base"
    if [ "$did_stash" = true ]; then git stash pop 2>/dev/null; fi
    exit 1
  fi
fi

if [ "$did_stash" = true ] && [ "$pop" = true ]; then
  echo "Restoring stashed changes..."
  git stash pop || echo "Warning: Conflicts while restoring stash. Resolve manually"
fi

echo "Done: branchup '$target' from latest '$base'"
