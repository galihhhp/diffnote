#!/bin/sh
set -u

base="main"
stash=true
pop=true
pull_rebase=false
did_stash=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-stash)
      stash=false
      pop=false
      shift
      ;;
    --no-pop)
      pop=false
      shift
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
      base="$1"
      shift
      ;;
  esac
done

if ! git show-ref --verify --quiet "refs/heads/$base"; then
  echo "Error: Branch '$base' doesn't exist locally"
  exit 1
fi

if [ "$stash" = true ]; then
  if [ -n "$(git status --porcelain)" ]; then
    echo "Stashing uncommitted changes..."
    git stash push -u -m "auto-stash before rebase" || exit 1
    did_stash=true
  fi
fi

git checkout "$base" || exit 1
if [ "$pull_rebase" = true ]; then
  echo "Pulling with rebase..."
  git pull --rebase origin "$base" || exit 1
else
  echo "Pulling with merge..."
  git pull origin "$base" || exit 1
fi
git checkout - || exit 1

if ! git rebase "$base"; then
  echo "Error: Conflict during rebase. Run 'git rebase --abort'"
  if [ "$did_stash" = true ] && [ "$pop" = true ]; then
    git stash pop
  fi
  exit 1
fi

if [ "$did_stash" = true ] && [ "$pop" = true ]; then
  echo "Restoring stashed changes..."
  git stash pop || echo "Warning: Conflicts while restoring stash"
fi

echo "Done: Rebased onto '$base'"
