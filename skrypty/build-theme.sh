#!/bin/bash
set -e

# ==============================================
#  GOST-OS THEME PACKAGE BUILDER (improved)
# ==============================================
# Buduje paczkę .deb z motywem graficznym GOST OS
# i umieszcza ją w katalogu config/packages.chroot,
# by live-build mógł ją automatycznie zainstalować.

THEME_SRC="gost-theme-package"
OUTPUT_DIR="config/packages.chroot"

echo "🔧 [GOST-OS] Rozpoczynam budowanie pakietu motywu..."
echo "Źródło: $THEME_SRC"
echo "Cel: $OUTPUT_DIR"
echo "----------------------------------------------"

# 1️⃣ Sprawdź, czy katalog źródłowy istnieje
if [ ! -d "$THEME_SRC" ]; then
    echo "❌ Błąd: brak katalogu $THEME_SRC"
    exit 1
fi

# 2️⃣ Usuń poprzednie buildy, jeśli istnieją
rm -f ./*.deb || true

# 3️⃣ Przejdź do katalogu pakietu
cd "$THEME_SRC"

# 4️⃣ Usuń stare buildy (czystość)
rm -rf build/ ../*.deb || true

# 5️⃣ Buduj pakiet (bez podpisywania)
echo "📦 Buduję pakiet .deb..."
dpkg-buildpackage -us -uc

# 6️⃣ Wróć do głównego katalogu repo
cd ..

# 7️⃣ Znajdź najnowszy zbudowany pakiet
DEB_FILE=$(ls -1t gost-theme-package_*.deb 2>/dev/null | head -n 1)

if [ -z "$DEB_FILE" ]; then
    echo "❌ Nie znaleziono pliku .deb! Budowa nie powiodła się."
    exit 1
fi

echo "✅ Zbudowano: $DEB_FILE"

# 8️⃣ Utwórz katalog docelowy (jeśli go brak)
mkdir -p "$OUTPUT_DIR"

# 9️⃣ Skopiuj .deb do packages.chroot/
cp "$DEB_FILE" "$OUTPUT_DIR/"

echo "✅ Skopiowano do: $OUTPUT_DIR/$DEB_FILE"
echo "----------------------------------------------"
echo "🎉 Gotowe! Motyw GOST OS będzie zainstalowany w systemie live."
