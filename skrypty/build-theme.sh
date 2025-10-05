#!/bin/bash
set -eux

# ==========================================================
#   GOST OS – Skrypt budujący pakiet .deb z motywem systemu
# ==========================================================

THEME_DIR="gost-theme-package"
OUTPUT_DIR="config/packages.chroot"

echo "🔧 [GOST-OS] Rozpoczynam budowanie pakietu motywu..."
echo "Źródło: $THEME_DIR"
echo "Cel: $OUTPUT_DIR"
echo "----------------------------------------------"

# 1️⃣ Sprawdź, czy katalog z motywem istnieje
if [ ! -d "$THEME_DIR" ]; then
  echo "❌ Nie znaleziono katalogu $THEME_DIR!"
  exit 1
fi

# 2️⃣ Przejdź do katalogu z motywem
cd "$THEME_DIR"

# 3️⃣ Nadaj uprawnienia do pliku rules
chmod +x debian/rules || true

# 4️⃣ Zbuduj pakiet (bez podpisywania)
echo "📦 Buduję pakiet .deb..."
dpkg-buildpackage -us -uc || {
  echo "❌ Błąd podczas budowania pakietu motywu."
  exit 1
}

# 5️⃣ Wróć do głównego katalogu
cd ..

# 6️⃣ Znajdź wygenerowany plik .deb i przenieś go do repozytorium
DEB_FILE=$(find . -maxdepth 1 -type f -name "*.deb" | head -n 1 || true)

if [ -n "$DEB_FILE" ]; then
  mkdir -p "$OUTPUT_DIR"
  mv "$DEB_FILE" "$OUTPUT_DIR/"
  echo "✅ Pakiet przeniesiony do $OUTPUT_DIR/"
else
  echo "❌ Nie znaleziono pliku .deb! Budowa nie powiodła się."
  exit 1
fi

echo "🎉 Pakiet motywu został zbudowany i przeniesiony pomyślnie."
