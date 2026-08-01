#!/usr/bin/env bash
# Repository hygiene: the repository holds the project and nothing else.
#
# Run by the pre-commit hook and by CI, so a rule cannot pass in one place and
# fail in the other. See docs/04-zasady-pracy.md, "Zasada czystego repozytorium".
#
# Everything checked here is permanent once committed: git history does not
# forget, and deleting a file in a later commit deletes nothing. That is the
# whole reason this runs before the commit exists rather than after.
#
# Usage:
#   scripts/higiena.sh            # check the whole tracked tree (CI)
#   scripts/higiena.sh --staged   # check what is about to be committed (hook)
set -uo pipefail

cd "$(git rev-parse --show-toplevel)" || exit 2

MODE="${1:-}"
if [ "$MODE" = "--staged" ]; then
    mapfile -t FILES < <(git diff --cached --name-only --diff-filter=ACMR)
else
    mapfile -t FILES < <(git ls-files)
fi

fail=0
report() {
    printf '\n  ✗ %s\n' "$1"
    shift
    printf '      %s\n' "$@"
    fail=1
}

# ── 1. Only the project's own paths are tracked ──────────────────────────────
# An allowlist, not a list of forbidden names: the leak we actually suffered was
# a temporary file nobody would have thought to forbid, swept in by `git add -A`.
# A list of what is allowed catches the file nobody anticipated; a list of what
# is banned catches only what somebody already thought of.
ALLOWED='^(crates/|docs/|resources/|scripts/|\.github/|Cargo\.(toml|lock)$|LICENSE$|README\.md$|\.gitignore$|\.dockerignore$|Dockerfile$|deny\.toml$|clippy\.toml$|rustfmt\.toml$|rust-toolchain\.toml$|gostos\.md$)'
for f in "${FILES[@]}"; do
    [ -z "$f" ] && continue
    if ! [[ "$f" =~ $ALLOWED ]]; then
        report "ścieżka spoza projektu: $f" \
            "Repozytorium trzyma projekt i nic poza nim. Jeśli plik ma tu być," \
            "dopisz go do ALLOWED w scripts/higiena.sh — świadomie, nie odruchowo."
    fi
done

# ── 2. No local paths in the content ─────────────────────────────────────────
# A local path says where somebody works, not what was built, and it is the
# thing that leaks first: it arrives inside a config file or a log fragment.
#
# Documentation and tests legitimately spell out example paths, so a home
# directory belonging to an obvious placeholder is allowed. The real name of a
# real account is not — and is not written down here either, because a check
# that names what it is hiding hides nothing.
PLACEHOLDER='^(user|u|users|uzytkownik|username|alice|bob|someone|example|home)$'
LOCAL_PATH='(/home/[A-Za-z_][A-Za-z0-9_.-]*/|/tmp/[a-z]+-[0-9]+/|/run/user/[0-9]+/)'
for f in "${FILES[@]}"; do
    [ -z "$f" ] || [ ! -f "$f" ] && continue
    [ "$f" = "scripts/higiena.sh" ] && continue  # this file quotes the patterns
    while IFS= read -r hit; do
        [ -z "$hit" ] && continue
        # /home/<name>/ with a placeholder name is an example, not a leak.
        name="$(printf '%s' "$hit" | grep -oE '/home/[A-Za-z_][A-Za-z0-9_.-]*/' | head -1 | cut -d/ -f3)"
        if [ -n "$name" ] && printf '%s' "$name" | grep -qE "$PLACEHOLDER"; then
            continue
        fi
        report "ścieżka lokalna w $f" "$hit"
    done < <(grep -nIE "$LOCAL_PATH" "$f" 2>/dev/null | head -3)
done

# ── 3. Images carry no metadata ──────────────────────────────────────────────
# Editors put an account id into XMP. It survives every later commit and ties
# the repository to a person's account in somebody else's service.
for f in "${FILES[@]}"; do
    [ -z "$f" ] && continue
    case "$f" in
        *.png|*.jpg|*.jpeg|*.webp)
            [ -f "$f" ] || continue
            # `grep -c`, not `grep -q`, and the reason is not style. Under
            # `pipefail`, `grep -q` exits at the first match and `strings` dies
            # of SIGPIPE with status 141 — so the pipeline reports failure and
            # the `if` reads it as "no metadata found". Whether that happens
            # depends on how much output fits in the pipe buffer, which is to
            # say the check passed on one machine and failed on another. `-c`
            # reads to the end, so there is no early exit and no race.
            hits=$(strings "$f" 2>/dev/null | grep -cE '<x:xmpmeta|<xmp:CreatorTool|photoshop:|Canva|Exif' || true)
            if [ "${hits:-0}" -gt 0 ]; then
                report "metadane w obrazie $f" \
                    "Przepisz obraz bez metadanych przed dodaniem." \
                    "python3 -c \"from PIL import Image; i=Image.open('$f'); o=Image.new(i.mode,i.size); o.putdata(list(i.getdata())); o.save('$f','PNG',optimize=True)\""
            fi
            ;;
    esac
done

# ── 4. Commits carry no private address ──────────────────────────────────────
# GitHub's noreply address is the only one that may appear. A private address in
# a public history is permanent and is harvested within days.
if [ "$MODE" = "--staged" ]; then
    who="$(git var GIT_AUTHOR_IDENT | sed -n 's/.*<\(.*\)>.*/\1/p')"
    case "$who" in
        *@users.noreply.github.com) ;;
        *) report "prywatny adres w autorze commita: $who" \
               "git config user.email '<login>@users.noreply.github.com'" ;;
    esac
fi

if [ "$fail" -ne 0 ]; then
    printf '\nHigiena repozytorium: ODRZUCONE. Usuń przyczynę, nie ostrzeżenie —\n'
    printf 'commit z takim plikiem jest nieodwracalny (docs/04-zasady-pracy.md).\n\n'
    exit 1
fi
printf 'Higiena repozytorium: czysto (%d plików).\n' "${#FILES[@]}"
