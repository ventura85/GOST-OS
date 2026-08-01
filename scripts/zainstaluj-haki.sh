#!/usr/bin/env bash
# Install the repository's git hooks.
#
# `.git/hooks/` is not part of a clone, so a fresh checkout has no protection
# until this runs. Run it once per machine, before the first commit.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
hook="$root/.git/hooks/pre-commit"

cat > "$hook" <<'EOF'
#!/usr/bin/env bash
# Installed by scripts/zainstaluj-haki.sh — do not edit here, edit the script.
exec "$(git rev-parse --show-toplevel)/scripts/higiena.sh" --staged
EOF
chmod +x "$hook"

echo "Zainstalowano: $hook"
echo "Sprawdzenie:   scripts/higiena.sh"
