#!/usr/bin/env bash
# Reproduce a Medium article demo from articles/catalog.toml.
#
# Usage:
#   ./scripts/reproduce-article.sh <slug>
#   ./scripts/reproduce-article.sh --list
#
# Does not modify git state. Warns when HEAD is not at the article's tag.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CATALOG="$ROOT/articles/catalog.toml"
cd "$ROOT"

die() { echo "error: $*" >&2; exit 1; }

list_slugs() {
  awk -F'"' '/^slug *=/ { print $2 }' "$CATALOG"
}

# Print the value for `key` inside the [[article]] block whose slug matches.
field_for_slug() {
  local slug="$1" key="$2"
  awk -v slug="$slug" -v key="$key" '
    BEGIN { match_slug = 0 }
    /^\[\[article\]\]/ {
      if (match_slug) exit
      match_slug = 0
      next
    }
    {
      line = $0
      sub(/#.*/, "", line)
      # trim
      gsub(/^[ \t]+|[ \t]+$/, "", line)
      if (line == "") next

      if (line ~ /^slug[ \t]*=/) {
        split(line, parts, "\"")
        if (parts[2] == slug) match_slug = 1
        next
      }

      if (!match_slug) next

      prefix = key "[ \t]*="
      if (line ~ ("^" prefix)) {
        if (index(line, "\"") > 0) {
          split(line, q, "\"")
          print q[2]
        } else {
          sub("^[^=]*=[ \t]*", "", line)
          gsub(/[ \t]+$/, "", line)
          print line
        }
        exit
      }
    }
  ' "$CATALOG"
}

print_article_banner() {
  local slug="$1"
  local title git_tag summary assets medium_url workload
  title="$(field_for_slug "$slug" title)"
  git_tag="$(field_for_slug "$slug" git_tag)"
  summary="$(field_for_slug "$slug" summary)"
  assets="$(field_for_slug "$slug" assets_dir)"
  medium_url="$(field_for_slug "$slug" medium_url)"
  workload="$(field_for_slug "$slug" workload)"

  echo "=== FlyBy Medium reproduce ==="
  echo "Slug     : $slug"
  echo "Title    : $title"
  echo "Summary  : $summary"
  echo "Git tag  : $git_tag"
  echo "Workload : $workload"
  if [[ -n "${medium_url}" ]]; then
    echo "Medium   : $medium_url"
  else
    echo "Medium   : (not published yet)"
  fi
  echo "Assets   : articles/${assets}/"
  echo "Expected : articles/${assets}/expected-output.md"
  echo "Screens  : articles/${assets}/screenshots/"
  echo

  if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    if git rev-parse -q --verify "refs/tags/${git_tag}" >/dev/null 2>&1; then
      local head tag_commit
      head="$(git rev-parse HEAD)"
      tag_commit="$(git rev-parse "refs/tags/${git_tag}^{commit}")"
      if [[ "$head" == "$tag_commit" ]]; then
        echo "Git      : HEAD matches ${git_tag}"
      else
        echo "Git      : WARNING — HEAD is not at ${git_tag}"
        echo "           Exact article numbers: git switch --detach ${git_tag}"
      fi
    else
      echo "Git      : tag ${git_tag} not present locally yet (ok while drafting)"
    fi
  fi
  echo
  echo "Note     : results are SIMULATED (not hardware)"
  echo
}

run_article() {
  local slug="$1"
  local workload scenario pcap full_speed
  workload="$(field_for_slug "$slug" workload)"
  [[ -n "$workload" ]] || die "unknown slug '$slug' (try --list)"

  print_article_banner "$slug"

  case "$workload" in
    scenario)
      scenario="$(field_for_slug "$slug" scenario)"
      [[ -n "$scenario" ]] || die "article $slug missing scenario="
      echo "\$ cargo run -p flyby-simulator --bin flyby-sim -- $scenario"
      echo
      cargo run -p flyby-simulator --bin flyby-sim -- "$scenario"
      ;;
    pcap)
      pcap="$(field_for_slug "$slug" pcap)"
      full_speed="$(field_for_slug "$slug" full_speed)"
      [[ -n "$pcap" ]] || die "article $slug missing pcap="
      [[ -f "$pcap" ]] || die "pcap not found: $pcap"
      local -a args
      args=(pcap "$pcap")
      if [[ "${full_speed:-true}" == "true" ]]; then
        args+=(--full-speed)
      fi
      echo "\$ cargo run -p flyby-simulator --bin flyby-sim -- ${args[*]}"
      echo
      cargo run -p flyby-simulator --bin flyby-sim -- "${args[@]}"
      ;;
    *)
      die "unsupported workload '$workload' for $slug"
      ;;
  esac
}

main() {
  [[ -f "$CATALOG" ]] || die "missing $CATALOG"

  if [[ "${1:-}" == "" || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    echo "Usage: $0 <slug>"
    echo "       $0 --list"
    exit 0
  fi

  if [[ "$1" == "--list" || "$1" == "list" ]]; then
    echo "Available articles:"
    while IFS= read -r slug; do
      title="$(field_for_slug "$slug" title)"
      printf "  %-28s  %s\n" "$slug" "$title"
    done < <(list_slugs)
    exit 0
  fi

  run_article "$1"
}

main "$@"
