#!/bin/bash
set -eux

# Przejdź do katalogu z pakietem motywu
cd gost-theme-package

# Zbuduj pakiet
# -us -uc zapobiega podpisywaniu pakietu, co jest niepotrzebne w tym kontekście
dpkg-buildpackage -us -uc

# Wróć do głównego katalogu
cd ..

# Przenieś zbudowany pakiet .deb do repozytorium dla live-build
# Używamy ls, aby znaleźć plik .deb, ponieważ jego nazwa może się zmieniać
mv *.deb config/packages.chroot/